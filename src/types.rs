use chrono::prelude::*;
use time::Duration;

// implements next/last direction in expressions like 'next friday' and 'last 4 july'
#[derive(Debug,Clone,Copy,PartialEq)]
pub enum Direction {
    Next,
    Last,
    Here
}

impl Direction {
    pub fn from_name(s: &str) -> Option<Direction> {
        use Direction::*;
        match s {
            "next" => Some(Next),
            "last" => Some(Last),
            _ => None
        }
    }
}

// this is a day-month with direction, like 'next 10 Dec'
#[derive(Debug)]
pub struct YearDate {
    pub direct: Direction,
    pub month: u32,
    pub day: u32,
}

// for expressions like 'friday' and 'July' modifiable with next/last
#[derive(Debug)]
pub struct NamedDate {
    pub direct: Direction,
    pub unit: u32
}

impl NamedDate {
    pub fn new(direct: Direction, unit: u32) -> NamedDate {
        NamedDate{direct: direct, unit: unit}
    }
}

// all expressions modifiable with next/last; 'fri', 'jul', '5 may'.
#[derive(Debug)]
pub enum ByName {
    WeekDay(NamedDate),
    MonthName(NamedDate),
    DayMonth(YearDate),
}

fn add_days<Tz: TimeZone>(base: Date<Tz>, days: i64) -> Option<Date<Tz>> {
    base.checked_add_signed(Duration::days(days))
}

fn next_last_direction<Tz: TimeZone>(date: Date<Tz>, base: Date<Tz>, direct: Direction) -> Option<i32> {
    let ahead = date > base;
    let mut res = None;
    if ahead {
        if direct == Direction::Last {
            res = Some(-1);
        }
    } else {
        if direct == Direction::Next {
            res = Some(1)
        }
    }
    res
}

impl ByName {
    pub fn from_name(s: &str, direct: Direction) -> Option<ByName> {
        Some(
            if let Some(wd) = week_day(s) {
                ByName::WeekDay(NamedDate::new(direct,wd))
            } else
            if let Some(mn) = month_name(s) {
                ByName::MonthName(NamedDate::new(direct,mn))
            } else {
                return None;
            }
        )
    }

    pub fn as_month(&self) -> Option<u32> {
        match *self {
            ByName::MonthName(ref nd) => Some(nd.unit),
            _ => None
        }
    }

    pub fn from_day_month(d: u32, m: u32, direct: Direction) -> ByName {
        ByName::DayMonth(YearDate{direct: direct, day: d, month: m})
    }

    pub fn to_date<Tz: TimeZone>(self, base: Date<Tz>) -> Option<Date<Tz>>
    where <Tz as TimeZone>::Offset: Copy {
        let this_year = base.year();
        match self {
            ByName::WeekDay(nd) => {
                let this_day = base.weekday().num_days_from_monday() as i64;
                let that_day = nd.unit as i64;
                let mut date = add_days(base,that_day - this_day)?;
                if let Some(correct) = next_last_direction(date,base,nd.direct) {
                    date = add_days(date,7*correct as i64)?;
                }
                Some(date)
            },
            ByName::MonthName(nd) => {
                let mut date = base.timezone().ymd_opt(this_year,nd.unit,1).single()?;
                if let Some(correct) = next_last_direction(date,base,nd.direct) {
                    date = base.timezone().ymd_opt(this_year + correct,nd.unit,1).single()?;
                }
                Some(date)
            },
            ByName::DayMonth(yd) => {
                let mut date = base.timezone().ymd_opt(this_year,yd.month,yd.day).single()?;
                if let Some(correct) = next_last_direction(date,base,yd.direct) {
                    date = base.timezone().ymd_opt(this_year + correct,yd.month,yd.day).single()?;
                }
                Some(date)
            }
        }
    }

}

#[derive(Debug)]
pub struct AbsDate {
    pub year: i32,
    pub month: u32,
    pub day: u32,
}

impl AbsDate {
    pub fn to_date<Tz: TimeZone>(self, base: DateTime<Tz>) -> Option<Date<Tz>> {
        base.timezone().ymd_opt(self.year, self.month, self.day).single()
    }
}

// Skipping a given number of time units.
// The subtlety is that we treat duration as seconds until we get
// to months, where we want to preserve dates. So adding a month to
// '5 May' gives '5 June'. Adding a month to '30 Jan' gives 'Feb 28' or 'Feb 29'
// depending on whether this is a leap year.
#[derive(Debug)]
pub enum Interval {
    Seconds(i32),
    Months(i32),
}

#[derive(Debug)]
pub struct Skip {
    pub unit: Interval,
    pub skip: i32,
}

impl Skip {
    pub fn to_date_time<Tz: TimeZone>(self, base: DateTime<Tz>, ts: TimeSpec) -> Option<DateTime<Tz>> {
        Some(match self.unit {
            Interval::Seconds(secs) => {
                base.checked_add_signed(
                    Duration::seconds((secs as i64)*(self.skip as i64))
                ).unwrap() // <--- !!!!
            },
            Interval::Months(mm) => {
                let (y,m0,d) = (base.year(), (base.month()-1) as i32, base.day());
                // our new month number
                let mm = m0 + mm*self.skip;
                // which may run over to the next year and so forth
                let (y,m) = (y + mm/12, mm%12 + 1);
                // let chrono work out if the result makes sense
                let mut date = base.timezone().ymd_opt(y,m as u32,d).single();
                // dud dates like Feb 30 may result, so we back off...
                let mut d = d;
                while date.is_none() {
                    d -= 1;
                    if d == 0 || d < 28 { // sanity check...
                        eprintln!("fkd date");
                        return None;
                    }
                    date = base.timezone().ymd_opt(y,m as u32,d).single();
                }
                ts.to_date_time(date.unwrap())?
            },
        })
    }
}

#[derive(Debug)]
pub enum DateSpec {
    Absolute(AbsDate), // Y M D (e.g. 2018-06-02, 4 July 2017)
    Relative(Skip), // n U (e.g. 2min, 3 years ago, -2d)
    FromName(ByName),  // (e.g. 'next fri', 'jul')
}

impl DateSpec {
    pub fn absolute(y: u32, m: u32, d: u32) -> DateSpec {
        DateSpec::Absolute(
            AbsDate{year: y as i32, month: m, day: d}
        )
    }

    pub fn from_day_month(d: u32, m: u32, direct: Direction) -> DateSpec {
        DateSpec::FromName(
            ByName::from_day_month(d,m,direct)
        )
    }

    pub fn skip(unit: Interval, n: i32) -> DateSpec {
        DateSpec::Relative(
           Skip{unit: unit, skip: n}
        )
    }

    pub fn to_date_time<Tz: TimeZone>(self, base: DateTime<Tz>, ts: TimeSpec) -> Option<DateTime<Tz>>
    where Tz::Offset: Copy {
        use DateSpec::*;
        match self {
            Absolute(ad) => ts.to_date_time(ad.to_date(base)?),
            Relative(skip) => skip.to_date_time(base,ts), // might need time
            FromName(byname) => ts.to_date_time(byname.to_date(base.date())?),
        }
    }
}

#[derive(Debug)]
pub struct TimeSpec {
    pub hour: u32,
    pub min: u32,
    pub sec: u32,
}

impl TimeSpec {
    pub fn new(hour: u32, min: u32, sec: u32) -> TimeSpec {
        TimeSpec{hour: hour, min: min, sec: sec}
    }

    pub fn to_date_time<Tz: TimeZone>(self, d: Date<Tz>) -> Option<DateTime<Tz>> {
        d.and_hms_opt(self.hour, self.min, self.sec)
    }
}

#[derive(Debug)]
pub struct DateTimeSpec {
    pub date: Option<DateSpec>,
    pub time: Option<TimeSpec>,
}

// same as chrono's 'count days from monday' convention
pub fn week_day(s: &str) -> Option<u32> {
    if s.len() < 3 { return None; }
    Some(match &s[0..3] {
        "sun" => 6,
        "mon" => 0,
        "tue" => 1,
        "wed" => 2,
        "thur" => 3,
        "fri" => 4,
        "sat" => 5,
        _ => return None
    })
}

pub fn month_name(s: &str) -> Option<u32> {
    if s.len() < 3 { return None; }
    Some(match &s[0..3] {
        "jan" => 1,
        "feb" => 2,
        "mar" => 3,
        "apr" => 4,
        "may" => 5,
        "jun" => 6,
        "jul" => 7,
        "aug" => 8,
        "sep" => 9,
        "nov" => 10,
        "oct" => 11,
        "dec" => 12,
        _ => return None
    })
}

pub fn time_unit(s: &str) -> Option<Interval> {
    use Interval::*;
    let name = if s.len() < 3 {
        match &s[0..1] {
            "s" => "sec",
            "m" => "min",
            "h" => "hou",
            "d" => "day",
            "y" => "yea",
            _ => return None
        }
    } else {
        &s[0..3]
    };
    Some(match name {
        "sec" => Seconds(1),
        "min" => Seconds(60),
        "hou" => Seconds(60*60),
        "day" => Seconds(60*60*24),
        "mon" => Months(1),
        "yea" => Months(12),
        _ => return None
    })
}



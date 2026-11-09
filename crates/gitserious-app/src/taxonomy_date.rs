use std::fmt::{self, Display, Formatter};
use std::str::FromStr;

use time::macros::format_description;
use time::{Date, OffsetDateTime, PrimitiveDateTime, Time};

/// A UTC calendar day, or a UTC instant truncated to a second, for a Library taxonomy.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct TaxonomyDate {
    date: Date,
    time: Option<ClockTime>,
}

/// A whole UTC second. Both constructors copy fields that are already in range.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct ClockTime {
    hour: u8,
    minute: u8,
    second: u8,
}

impl ClockTime {
    fn from_clock(now: OffsetDateTime) -> Self {
        Self {
            hour: now.hour(),
            minute: now.minute(),
            second: now.second(),
        }
    }

    fn from_time(time: Time) -> Self {
        Self {
            hour: time.hour(),
            minute: time.minute(),
            second: time.second(),
        }
    }
}

impl TaxonomyDate {
    /// The current UTC instant, truncated to a whole second.
    #[must_use]
    pub fn now_utc() -> Self {
        let now = OffsetDateTime::now_utc();
        Self {
            date: now.date(),
            time: Some(ClockTime::from_clock(now)),
        }
    }
}

impl FromStr for TaxonomyDate {
    type Err = time::error::Parse;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value.contains(' ') {
            let parsed = PrimitiveDateTime::parse(
                value,
                format_description!("[year]-[month]-[day] [hour]:[minute]:[second] UTC"),
            )?;
            return Ok(Self {
                date: parsed.date(),
                time: Some(ClockTime::from_time(parsed.time())),
            });
        }
        Date::parse(value, format_description!("[year]-[month]-[day]"))
            .map(|date| Self { date, time: None })
    }
}

impl Display for TaxonomyDate {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self.time {
            None => self.date.fmt(formatter),
            Some(time) => write!(
                formatter,
                "{} {:02}:{:02}:{:02} UTC",
                self.date, time.hour, time.minute, time.second
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::TaxonomyDate;

    type Result = std::result::Result<(), time::error::Parse>;

    #[test]
    fn a_calendar_day_stays_a_calendar_day() -> Result {
        let day: TaxonomyDate = "2026-11-07".parse()?;
        assert_eq!(day.to_string(), "2026-11-07");
        Ok(())
    }

    #[test]
    fn a_utc_timestamp_keeps_the_calendar_day_and_the_time() -> Result {
        let stamp: TaxonomyDate = "2026-11-07 15:04:05 UTC".parse()?;
        assert_eq!(stamp.to_string(), "2026-11-07 15:04:05 UTC");
        Ok(())
    }

    #[test]
    fn now_utc_round_trips_with_the_zone() -> Result {
        let now = TaxonomyDate::now_utc();
        let text = now.to_string();
        assert!(text.ends_with(" UTC"), "{text}");
        assert_eq!(text.parse::<TaxonomyDate>()?, now);
        Ok(())
    }
}

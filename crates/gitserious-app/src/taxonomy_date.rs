use std::fmt::{self, Display, Formatter};
use std::str::FromStr;

use time::macros::format_description;
use time::{Date, OffsetDateTime};

/// A UTC calendar date recorded for a Library taxonomy.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct TaxonomyDate(Date);

impl TaxonomyDate {
    /// Reads the current UTC date at the persistence boundary.
    #[must_use]
    pub fn today_utc() -> Self {
        Self(OffsetDateTime::now_utc().date())
    }
}

impl FromStr for TaxonomyDate {
    type Err = time::error::Parse;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Date::parse(value, format_description!("[year]-[month]-[day]")).map(Self)
    }
}

impl Display for TaxonomyDate {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

#[cfg(test)]
mod tests {
    use super::TaxonomyDate;

    #[test]
    fn a_calendar_day_stays_a_calendar_day() {
        let day: TaxonomyDate = "2026-11-07".parse().unwrap();
        assert_eq!(day.to_string(), "2026-11-07");
    }

    #[test]
    fn a_utc_timestamp_keeps_the_calendar_day_and_the_time() {
        let stamp: TaxonomyDate = "2026-11-07 15:04:05 UTC".parse().unwrap();
        assert_eq!(stamp.to_string(), "2026-11-07 15:04:05 UTC");
    }
}

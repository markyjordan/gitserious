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

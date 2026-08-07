use std::error::Error;
use std::fmt::{self, Display, Formatter};

use gitserious_core::{IdentifierError, TemplateId};

/// The only project-configuration format understood by this release.
pub const PROJECT_CONFIG_VERSION: u16 = 1;

/// Authored repository-local policy selection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectConfig {
    version: u16,
    active_template: TemplateId,
}

impl ProjectConfig {
    /// Creates a supported project configuration.
    ///
    /// # Errors
    ///
    /// Returns [`ProjectConfigError`] when the format version is unsupported.
    pub fn new(version: u16, active_template: TemplateId) -> Result<Self, ProjectConfigError> {
        if version != PROJECT_CONFIG_VERSION {
            return Err(ProjectConfigError::UnsupportedVersion(version));
        }
        Ok(Self {
            version,
            active_template,
        })
    }

    /// Creates the initial configuration selecting the moving `default` channel.
    ///
    /// # Errors
    ///
    /// Returns [`IdentifierError`] only if the built-in channel identifier is
    /// inconsistent with the core identifier contract.
    pub fn default_channel() -> Result<Self, IdentifierError> {
        let active_template = TemplateId::new("default")?;
        Ok(Self {
            version: PROJECT_CONFIG_VERSION,
            active_template,
        })
    }

    /// Returns the project-configuration format version.
    #[must_use]
    pub const fn version(&self) -> u16 {
        self.version
    }

    /// Returns the authored template reference.
    #[must_use]
    pub const fn active_template(&self) -> &TemplateId {
        &self.active_template
    }
}

/// An unsupported project-configuration format.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProjectConfigError {
    /// The file declares a version this binary cannot interpret.
    UnsupportedVersion(u16),
}

impl Display for ProjectConfigError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedVersion(version) => {
                write!(formatter, "unsupported project config version {version}")
            }
        }
    }
}

impl Error for ProjectConfigError {}

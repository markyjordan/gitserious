use crate::{ProjectConfig, ProjectLock};

/// Repository-local project state observed before initialization.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProjectState {
    /// Neither known project file exists.
    Absent,
    /// Authored configuration exists but its generated lock is missing.
    ConfigOnly(ProjectConfig),
    /// Both authored configuration and generated lock exist.
    Initialized {
        /// The authored project configuration.
        config: ProjectConfig,
        /// The generated resolved policy.
        lock: ProjectLock,
    },
    /// A generated lock exists without authored configuration.
    LockOnly,
}

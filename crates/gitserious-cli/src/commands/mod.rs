use clap::Subcommand;

#[derive(Debug, Subcommand)]
pub(crate) enum Command {
    /// Initialize repository-local gitserious policy.
    ///
    /// Init writes `gitserious.toml` at the root of the current Git repository.
    /// It holds complete copies of the taxonomies your Library gives new
    /// projects. The Library's default taxonomy becomes the project default.
    ///
    /// An existing `gitserious.toml` is never rewritten.
    Init,
    /// Interactively configure taxonomy policy.
    ///
    /// Config opens in the terminal with two sections. Project shows the
    /// taxonomies installed in this repository's `gitserious.toml` and does
    /// not change them. In a repository without one, it offers to initialize
    /// the project. Library edits the taxonomies in your global
    /// configuration, which new projects copy.
    ///
    /// While you edit a Library item, Ctrl+W saves it and Esc discards the
    /// edit. Copies, reorders, and confirmed deletions save at once.
    Config,
    /// Author and create a durable commit from the staged index.
    ///
    /// Commit opens the Commit page. From that page you start authoring,
    /// choose a type, write its properties, and review the exact message
    /// before gitserious passes it to `git commit`.
    ///
    /// Only staged changes are committed. Stage them with `git add` first.
    Commit,
    /// Display documentation for a command
    Help {
        /// The command to document
        command: Option<String>,
    },
}

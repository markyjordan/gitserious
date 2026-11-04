use std::error::Error;
use std::ffi::OsString;
use std::fmt::{self, Display, Formatter};
use std::io::{IsTerminal, Write};
use std::path::Path;
use std::process::ExitCode;

use clap::{CommandFactory, Parser, Subcommand, ValueEnum};
use gitserious_app::{
    CommitDraftAuthor, CommitDraftAuthorOutcome, CommitOutcome, CommitOutput, CommitWriter,
    ConfigurationEditor, GlobalConfigurationStore, HelpPage, InitOutcome, InitStatus,
    InteractiveEntry, InteractiveUi, ProjectConfigurationStore, ProjectVerification,
    RepositoryLocator, RepositoryRoot, RepositoryStatusReader, TaxonomyOrigin, VerifyProjectError,
    create_commit_with_taxonomy, edit_configuration, fingerprint_project_configuration,
    fingerprint_taxonomy, initialize_project, run_interactive, verify_project_configuration,
};
use gitserious_core::{
    CommitMessage, CommitTypeDefinition, CommitTypeId, PropertyRequirement, Taxonomy, TaxonomyId,
    VersionedTaxonomy,
};

#[derive(Debug, Parser)]
#[command(
    name = "gitserious",
    version,
    about = "Create durable commit-message policy for Git repositories",
    disable_colored_help = true,
    subcommand_required = false
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Author and create a durable commit from the staged index.
    Commit {
        /// Select a taxonomy for this commit without changing project policy.
        #[arg(long, value_name = "TAXONOMY")]
        taxonomy: Option<TaxonomyId>,
        /// Select a commit type without opening the terminal picker.
        #[arg(long = "type", value_name = "COMMIT TYPE")]
        commit_type: Option<CommitTypeId>,
    },
    /// Initialize repository-local gitserious policy.
    Init {
        /// Select an installed taxonomy for fresh project policy.
        #[arg(long, value_name = "TAXONOMY")]
        taxonomy: Option<TaxonomyId>,
    },
    /// Inspect or interactively configure taxonomy policy.
    Config {
        #[command(subcommand)]
        action: Option<ConfigAction>,
    },
}

#[derive(Debug, Subcommand)]
enum ConfigAction {
    /// List every installed taxonomy.
    List,
    /// Show one complete taxonomy.
    Show {
        /// Taxonomy identifier.
        taxonomy: TaxonomyId,
    },
    /// Check gitserious.toml without writing anything or reading the Library.
    Verify {
        /// Verify the repository's gitserious.toml.
        #[arg(long, required = true)]
        project: bool,
        /// Print a sentence for people or a JSON report for automation.
        #[arg(long, value_enum, default_value_t = ReportFormat::Human)]
        format: ReportFormat,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum ReportFormat {
    Human,
    Json,
}

/// Concrete interaction adapters required by the binary.
#[derive(Clone, Copy)]
pub struct CommitAdapters<'a, A: ?Sized, W: ?Sized> {
    author: &'a A,
    writer: &'a W,
    configuration_editor: Option<&'a dyn ConfigurationEditor>,
    interactive: Option<(&'a dyn InteractiveUi, &'a dyn RepositoryStatusReader)>,
}

impl<'a, A: ?Sized, W: ?Sized> CommitAdapters<'a, A, W> {
    #[must_use]
    pub const fn new(author: &'a A, writer: &'a W) -> Self {
        Self {
            author,
            writer,
            configuration_editor: None,
            interactive: None,
        }
    }

    /// Supplies the unified shell and its read-only Git adapter.
    #[must_use]
    pub fn with_interactive(
        mut self,
        ui: &'a dyn InteractiveUi,
        status: &'a dyn RepositoryStatusReader,
    ) -> Self {
        self.interactive = Some((ui, status));
        self
    }

    #[must_use]
    pub fn with_configuration_editor(mut self, editor: &'a dyn ConfigurationEditor) -> Self {
        self.configuration_editor = Some(editor);
        self
    }
}

/// Runs from the process current directory without interactive adapters.
#[must_use]
pub fn run<I, T, L, S, G, Out, Err>(
    arguments: I,
    locator: &L,
    store: &S,
    global: &G,
    stdout: &mut Out,
    stderr: &mut Err,
) -> ExitCode
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
    L: RepositoryLocator + ?Sized,
    L::Error: Display,
    S: ProjectConfigurationStore + ?Sized,
    S::Error: Display,
    G: GlobalConfigurationStore + ?Sized,
    G::Error: Display,
    Out: Write + ?Sized,
    Err: Write + ?Sized,
{
    let unavailable = UnsupportedCommitAdapter;
    run_with_commit(
        arguments,
        locator,
        store,
        global,
        &CommitAdapters::new(&unavailable, &unavailable),
        stdout,
        stderr,
    )
}

/// Runs from the process current directory with concrete adapters.
#[must_use]
#[allow(clippy::too_many_arguments)]
pub fn run_with_commit<I, T, L, S, G, A, W, Out, Err>(
    arguments: I,
    locator: &L,
    store: &S,
    global: &G,
    adapters: &CommitAdapters<'_, A, W>,
    stdout: &mut Out,
    stderr: &mut Err,
) -> ExitCode
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
    L: RepositoryLocator + ?Sized,
    L::Error: Display,
    S: ProjectConfigurationStore + ?Sized,
    S::Error: Display,
    G: GlobalConfigurationStore + ?Sized,
    G::Error: Display,
    A: CommitDraftAuthor + ?Sized,
    A::Error: Display,
    W: CommitWriter + ?Sized,
    W::Error: Display,
    Out: Write + ?Sized,
    Err: Write + ?Sized,
{
    let start = match std::env::current_dir() {
        Ok(path) => path,
        Err(error) => return operational_error(stderr, error),
    };
    run_from_with_commit(
        arguments, &start, locator, store, global, adapters, stdout, stderr,
    )
}

/// Runs from an explicit directory without interactive adapters.
#[must_use]
pub fn run_from<I, T, L, S, G, Out, Err>(
    arguments: I,
    start: &Path,
    locator: &L,
    store: &S,
    global: &G,
    stdout: &mut Out,
    stderr: &mut Err,
) -> ExitCode
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
    L: RepositoryLocator + ?Sized,
    L::Error: Display,
    S: ProjectConfigurationStore + ?Sized,
    S::Error: Display,
    G: GlobalConfigurationStore + ?Sized,
    G::Error: Display,
    Out: Write + ?Sized,
    Err: Write + ?Sized,
{
    let unavailable = UnsupportedCommitAdapter;
    run_from_with_commit(
        arguments,
        start,
        locator,
        store,
        global,
        &CommitAdapters::new(&unavailable, &unavailable),
        stdout,
        stderr,
    )
}

/// Runs from an explicit directory with concrete adapters.
#[must_use]
#[allow(clippy::too_many_arguments)]
pub fn run_from_with_commit<I, T, L, S, G, A, W, Out, Err>(
    arguments: I,
    start: &Path,
    locator: &L,
    store: &S,
    global: &G,
    adapters: &CommitAdapters<'_, A, W>,
    stdout: &mut Out,
    stderr: &mut Err,
) -> ExitCode
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
    L: RepositoryLocator + ?Sized,
    L::Error: Display,
    S: ProjectConfigurationStore + ?Sized,
    S::Error: Display,
    G: GlobalConfigurationStore + ?Sized,
    G::Error: Display,
    A: CommitDraftAuthor + ?Sized,
    A::Error: Display,
    W: CommitWriter + ?Sized,
    W::Error: Display,
    Out: Write + ?Sized,
    Err: Write + ?Sized,
{
    let cli = match Cli::try_parse_from(arguments) {
        Ok(cli) => cli,
        Err(error) => {
            let exit = exit_code(error.exit_code());
            let result = if error.use_stderr() {
                write!(stderr, "{error}")
            } else {
                write!(stdout, "{error}")
            };
            return if result.is_ok() {
                exit
            } else {
                ExitCode::FAILURE
            };
        }
    };
    let entry = match &cli.command {
        None => Some(InteractiveEntry::Home),
        Some(Command::Config { action: None }) if adapters.interactive.is_some() => {
            Some(InteractiveEntry::Config)
        }
        _ => None,
    };
    if let Some(entry) = entry {
        if entry == InteractiveEntry::Home
            && (!std::io::stdin().is_terminal() || !std::io::stdout().is_terminal())
        {
            return if writeln!(stdout, "{}", Cli::command().render_long_help()).is_ok() {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            };
        }
        let Some((ui, status)) = adapters.interactive else {
            return operational_error(stderr, "interactive shell is not configured");
        };
        return match run_interactive(
            locator,
            store,
            global,
            status,
            adapters.writer,
            start,
            ui,
            entry,
            &interactive_help(),
        ) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => operational_error(stderr, error),
        };
    }
    let Some(command) = cli.command else {
        return ExitCode::SUCCESS;
    };
    match command {
        Command::Commit {
            taxonomy,
            commit_type,
        } => match create_commit_with_taxonomy(
            locator,
            store,
            adapters.author,
            adapters.writer,
            start,
            taxonomy.as_ref(),
            commit_type.as_ref(),
        ) {
            Ok(CommitOutcome::Created(output)) => write_commit_output(stdout, stderr, &output),
            Ok(CommitOutcome::Cancelled) => {
                let _ = writeln!(stderr, "Commit cancelled.");
                ExitCode::FAILURE
            }
            Err(error) => operational_error(stderr, error),
        },
        Command::Init { taxonomy } => {
            let global = match global.load() {
                Ok(value) => value,
                Err(error) => return operational_error(stderr, error),
            };
            match initialize_project(locator, store, &global, taxonomy.as_ref(), start) {
                Ok(outcome) => write_init_outcome(stdout, &outcome),
                Err(error) => operational_error(stderr, error),
            }
        }
        Command::Config { action: None } => {
            let Some(editor) = adapters.configuration_editor else {
                return operational_error(stderr, "configuration editor is not configured");
            };
            match edit_configuration(locator, store, global, start, editor) {
                Ok(()) => ExitCode::SUCCESS,
                Err(error) => operational_error(stderr, error),
            }
        }
        Command::Config {
            action: Some(action),
        } => {
            let library = || {
                let global = global.load().map_err(|error| error.to_string())?;
                let catalog = global.catalog().map_err(|error| error.to_string())?;
                Ok::<_, String>((global, catalog))
            };
            match action {
                ConfigAction::Verify { format, .. } => {
                    let result = verify_project_configuration(locator, store, start);
                    match format {
                        ReportFormat::Human => write_verification(stdout, stderr, result),
                        ReportFormat::Json => write_verification_report(stdout, &result),
                    }
                }
                ConfigAction::List => {
                    let (global, catalog) = match library() {
                        Ok(value) => value,
                        Err(error) => return operational_error(stderr, error),
                    };
                    for taxonomy in catalog.taxonomies() {
                        let origin = catalog
                            .origin(taxonomy.id())
                            .map_or("unknown", TaxonomyOrigin::as_str);
                        let default = if taxonomy.id() == global.default_taxonomy() {
                            " default"
                        } else {
                            ""
                        };
                        let available = if global.available_taxonomies().contains(taxonomy.id()) {
                            " available"
                        } else {
                            ""
                        };
                        if writeln!(
                            stdout,
                            "{}\t{}\tv{}{}{}",
                            taxonomy.id(),
                            origin,
                            taxonomy.version(),
                            default,
                            available
                        )
                        .is_err()
                        {
                            return ExitCode::FAILURE;
                        }
                    }
                    ExitCode::SUCCESS
                }
                ConfigAction::Show { taxonomy } => match library() {
                    Err(error) => operational_error(stderr, error),
                    Ok((_, catalog)) => match catalog.find(&taxonomy) {
                        Some(taxonomy) => {
                            if write!(stdout, "{}", render_taxonomy(taxonomy)).is_ok() {
                                ExitCode::SUCCESS
                            } else {
                                ExitCode::FAILURE
                            }
                        }
                        None => operational_error(
                            stderr,
                            format!("taxonomy \"{taxonomy}\" is not installed"),
                        ),
                    },
                },
            }
        }
    }
}

fn render_taxonomy(taxonomy: &Taxonomy) -> String {
    let mut output = format!(
        "{}\nv{}\n{}\n",
        taxonomy.id(),
        taxonomy.version(),
        taxonomy.description()
    );
    if let Some(source) = taxonomy.derived_from() {
        output.push_str(&format!("forked from {}\n", source.versioned()));
    }
    for definition in taxonomy.commit_types() {
        output.push_str(&format!(
            "\n{}\t{}\n",
            definition.id(),
            definition.description()
        ));
        for property in definition.properties() {
            let requirement = match property.requirement() {
                PropertyRequirement::Required => "required".to_owned(),
                PropertyRequirement::Recommended => "recommended".to_owned(),
                PropertyRequirement::Optional => "optional".to_owned(),
                PropertyRequirement::Conditional(condition) => {
                    format!("conditional:{}", condition.id())
                }
            };
            output.push_str(&format!("  {}\t{requirement}\n", property.key()));
        }
    }
    output
}

fn write_commit_output(
    stdout: &mut (impl Write + ?Sized),
    stderr: &mut (impl Write + ?Sized),
    output: &CommitOutput,
) -> ExitCode {
    if stdout.write_all(output.stdout()).is_ok() && stderr.write_all(output.stderr()).is_ok() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn write_init_outcome(output: &mut (impl Write + ?Sized), outcome: &InitOutcome) -> ExitCode {
    let action = match outcome.status() {
        InitStatus::Initialized => "Initialized gitserious in",
        InitStatus::AlreadyInitialized => "gitserious is already initialized in",
    };
    if writeln!(
        output,
        "{action} {} ({}).",
        outcome.root().as_path().display(),
        VersionedTaxonomy::new(outcome.taxonomy(), outcome.version())
    )
    .is_ok()
    {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// `config verify` exits 0 when valid, 1 when invalid or absent, and 3 when the file cannot be read.
const VERIFY_ENVIRONMENT_ERROR: u8 = 3;

fn write_verification<L: Display, S: Display>(
    stdout: &mut (impl Write + ?Sized),
    stderr: &mut (impl Write + ?Sized),
    result: Result<ProjectVerification, VerifyProjectError<L, S>>,
) -> ExitCode {
    match result {
        Ok(ProjectVerification::Valid(configuration)) => {
            let taxonomies = configuration
                .taxonomies()
                .iter()
                .map(|taxonomy| taxonomy.versioned().to_string())
                .collect::<Vec<_>>()
                .join(", ");
            if writeln!(
                stdout,
                "gitserious.toml is valid.\ndefault: {}\ntaxonomies: {taxonomies}",
                configuration.default_taxonomy()
            )
            .is_ok()
            {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            }
        }
        Ok(ProjectVerification::NotInitialized) => operational_error(
            stderr,
            "gitserious is not initialized; gitserious.toml is missing",
        ),
        Ok(ProjectVerification::Invalid(message)) => operational_error(stderr, message),
        Err(error) => {
            let _ = writeln!(stderr, "error: {error}");
            ExitCode::from(VERIFY_ENVIRONMENT_ERROR)
        }
    }
}

/// The version of the JSON shape `config verify --format json` prints.
const VERIFY_REPORT_VERSION: u16 = 1;

/// Prints the machine-readable report; the exit codes match the human format.
fn write_verification_report<L: Display, S: Display>(
    stdout: &mut (impl Write + ?Sized),
    result: &Result<ProjectVerification, VerifyProjectError<L, S>>,
) -> ExitCode {
    let (exit, valid, project, diagnostic) = match result {
        Ok(ProjectVerification::Valid(configuration)) => {
            (ExitCode::SUCCESS, true, Some(configuration), None)
        }
        Ok(ProjectVerification::NotInitialized) => (
            ExitCode::FAILURE,
            false,
            None,
            Some((
                "not-initialized",
                "gitserious is not initialized; gitserious.toml is missing".to_owned(),
            )),
        ),
        Ok(ProjectVerification::Invalid(message)) => (
            ExitCode::FAILURE,
            false,
            None,
            Some(("invalid-configuration", message.clone())),
        ),
        Err(error) => (
            ExitCode::from(VERIFY_ENVIRONMENT_ERROR),
            false,
            None,
            Some(("unreadable", error.to_string())),
        ),
    };
    let mut report =
        format!("{{\n  \"report_version\": {VERIFY_REPORT_VERSION},\n  \"valid\": {valid},\n");
    match project {
        Some(configuration) => {
            let taxonomies = configuration
                .taxonomies()
                .iter()
                .map(|taxonomy| {
                    format!(
                        "    {{ \"id\": {}, \"version\": {}, \"fingerprint\": {} }}",
                        json_string(taxonomy.id().as_str()),
                        taxonomy.version(),
                        json_string(&fingerprint_taxonomy(taxonomy).to_string()),
                    )
                })
                .collect::<Vec<_>>()
                .join(",\n");
            report.push_str(&format!(
                "  \"project_config_version\": {},\n  \"default_taxonomy\": {},\n  \"configuration_fingerprint\": {},\n  \"taxonomies\": [\n{taxonomies}\n  ],\n",
                configuration.version(),
                json_string(configuration.default_taxonomy().as_str()),
                json_string(&fingerprint_project_configuration(configuration).to_string()),
            ));
        }
        None => report.push_str(
            "  \"project_config_version\": null,\n  \"default_taxonomy\": null,\n  \"configuration_fingerprint\": null,\n  \"taxonomies\": [],\n",
        ),
    }
    match diagnostic {
        Some((code, message)) => report.push_str(&format!(
            "  \"diagnostics\": [\n    {{ \"code\": {}, \"message\": {} }}\n  ]\n}}\n",
            json_string(code),
            json_string(&message),
        )),
        None => report.push_str("  \"diagnostics\": []\n}\n"),
    }
    if write!(stdout, "{report}").is_ok() {
        exit
    } else {
        ExitCode::FAILURE
    }
}

/// Quotes `value` as a JSON string.
fn json_string(value: &str) -> String {
    let mut quoted = String::with_capacity(value.len() + 2);
    quoted.push('"');
    for character in value.chars() {
        match character {
            '"' => quoted.push_str("\\\""),
            '\\' => quoted.push_str("\\\\"),
            '\n' => quoted.push_str("\\n"),
            '\r' => quoted.push_str("\\r"),
            '\t' => quoted.push_str("\\t"),
            control if u32::from(control) < 0x20 => {
                quoted.push_str(&format!("\\u{:04x}", u32::from(control)));
            }
            other => quoted.push(other),
        }
    }
    quoted.push('"');
    quoted
}

fn operational_error(output: &mut (impl Write + ?Sized), error: impl Display) -> ExitCode {
    let _ = writeln!(output, "error: {error}");
    ExitCode::FAILURE
}

fn exit_code(code: i32) -> ExitCode {
    u8::try_from(code).map_or(ExitCode::FAILURE, ExitCode::from)
}

#[derive(Clone, Copy, Debug)]
struct UnsupportedCommitAdapter;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct UnsupportedCommitError;

impl Display for UnsupportedCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str("interactive commit adapters are not configured")
    }
}

impl Error for UnsupportedCommitError {}

impl CommitDraftAuthor for UnsupportedCommitAdapter {
    type Error = UnsupportedCommitError;
    fn author(
        &self,
        _definitions: &[CommitTypeDefinition],
        _preselected: Option<&CommitTypeDefinition>,
    ) -> Result<CommitDraftAuthorOutcome, Self::Error> {
        Err(UnsupportedCommitError)
    }
}

impl CommitWriter for UnsupportedCommitAdapter {
    type Error = UnsupportedCommitError;
    fn commit(
        &self,
        _root: &RepositoryRoot,
        _message: &CommitMessage,
    ) -> Result<CommitOutput, Self::Error> {
        Err(UnsupportedCommitError)
    }
}

/// Man-style presentation data for the unified Help surface.
#[must_use]
pub fn interactive_help() -> Vec<HelpPage> {
    let commands = [
        ("Overview", Cli::command()),
        (
            "Config",
            Cli::command()
                .find_subcommand("config")
                .cloned()
                .unwrap_or_else(Cli::command),
        ),
        (
            "Commit",
            Cli::command()
                .find_subcommand("commit")
                .cloned()
                .unwrap_or_else(Cli::command),
        ),
        (
            "Init",
            Cli::command()
                .find_subcommand("init")
                .cloned()
                .unwrap_or_else(Cli::command),
        ),
    ];
    let descriptions = [
        "Bare gitserious opens Home. Commit previews Git changes before authoring a durable message. Config configures repository policy and your taxonomy library.",
        "Bare gitserious config opens Config. Project owns the complete taxonomies in gitserious.toml. Library owns taxonomies. Esc validates and saves Library edits. Copies, reorders, and confirmed deletions save immediately. Project changes use review and Apply. config verify --project checks gitserious.toml without writing or reading the Library; it exits 0 when valid, 1 when invalid or missing, 2 for usage errors, and 3 when the file cannot be read.",
        "gitserious commit authors directly from persisted project policy. --taxonomy chooses an available taxonomy for this commit; --type preselects a type. Only Git's staged index is committed. Stage changes with Git before using Start commit in the shell.",
        "gitserious init writes gitserious.toml with complete copies of your Library's default taxonomies. --taxonomy sets the project default. An existing gitserious.toml is never rewritten.",
    ];
    let examples = [
        "gitserious\ngitserious --help",
        "gitserious config\ngitserious config list\ngitserious config show conventional\ngitserious config verify --project",
        "git add src/\ngitserious commit\ngitserious commit --taxonomy conventional --type feat",
        "gitserious init\ngitserious init --taxonomy research",
    ];
    let mut pages: Vec<_> = commands.into_iter().enumerate().map(|(index, (title, mut command))| {
        if index > 0 { command = command.bin_name(format!("gitserious {}", title.to_lowercase())); }
        HelpPage {
            title: title.into(),
            text: format!("NAME\n    gitserious — {title}\n\nSYNOPSIS AND OPTIONS\n{}\nDESCRIPTION\n{}\n\nEXAMPLES\n{}\n\nEXIT STATUS\n    0: success or shell exit. Nonzero: an operational error or cancelled direct commit.\n", command.render_long_help(), descriptions[index], examples[index]),
        }
    }).collect();
    pages.push(HelpPage { title: "Keyboard Navigation".into(), text: "NAME\n    gitserious — keyboard navigation\n\nDESCRIPTION\n    Tab / Shift+Tab: switch sections from any overview, including Config's Project and Library.\n    Esc: close an overlay, go back, or return to Home from an overview.\n    Enter: open the selected item or start a staged commit.\n    /: contextual commands. ?: contextual help. q: quit at overview level.\n\nCONFIG\n    Menu: Up/Down selects Project or Library; Left/Right moves focus between Menu and its widgets.\n    Library: n creates, e edits, f forks, and d deletes a taxonomy.\n    Library NAV: Enter opens or closes a node; arrows move rows or panel focus.\n    Library EDIT: Enter and Up/Down move fields; Esc validates and saves. Correct inline errors before leaving EDIT.\n    Tab, navigation clicks, and quit cannot leave Library EDIT. Esc removes a new item only while it still equals its initial blank values.\n    Library copies, reorders, and confirmed deletions save immediately. A taxonomy needs a type before it can be included in a project.\n    Project: Ctrl+S reviews changes; Enter applies from Review.\n\nCOMMIT\n    Overview: r refreshes; o shows the full Git result; Left/Right controls directories.\n    Picker: Tab / Shift+Tab selects taxonomy; Up/Down selects type; Enter composes.\n    Composer: Ctrl+S validates and reviews; Esc returns through discard protection.\n    Review: Enter submits; Esc returns to editing.\n\nHELP\n    Left/Right selects pages; Up/Down and Page Up/Page Down scroll.\n".into() });
    pages
}

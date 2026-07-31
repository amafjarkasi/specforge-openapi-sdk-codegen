//! `specforge` -- CLI entry point.
//!
//! Reads an OpenAPI YAML/JSON spec, runs the full pipeline (parse -> resolve ->
//! IR -> emit), and writes a ready-to-build SDK for the chosen target language.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{bail, Context, Result};
use clap::{Args, Parser, Subcommand, ValueEnum};
use serde_json::Value as JsonValue;
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

use specforge_core::{
    apply_versioning, diff, generate_changelog, lint, lint_config, merge_specs, parse_file,
    profile_api, resolve, resolve_spec_path, scan_versions, ChangelogFormat, ChangelogOptions,
    DiffSeverity, LintConfig, MarketplaceIndex, PluginIndex, ProfileOptions, RuleSeverity,
    Severity, SpecforgeConfig, VersionStrategy, VersioningConfig,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

/// Target language for the generated SDK.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Default)]
enum Lang {
    /// TypeScript (native fetch, ESM/CJS dual package).
    #[default]
    Ts,
    /// Go (stdlib net/http).
    Go,
    /// Rust (reqwest + serde).
    Rust,
}

/// Target OpenAPI version for conversion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum OpenApiVersion {
    /// OpenAPI 3.0.x
    #[value(name = "3.0")]
    V30,
    /// OpenAPI 3.1.x
    #[value(name = "3.1")]
    V31,
}

/// Generate a typed SDK from an OpenAPI YAML/JSON spec.
#[derive(Parser, Debug)]
#[command(name = "specforge", version, about = "Generate and lint OpenAPI specs", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Generate an SDK from an OpenAPI spec.
    Generate(GenerateArgs),
    /// Lint and validate an OpenAPI spec without generating.
    Check(CheckArgs),
    /// Compare two OpenAPI specs and report breaking changes.
    Diff(DiffArgs),
    /// Emit the resolved IR as JSON (for external emitters / plugins).
    Emit(EmitArgs),
    /// Scaffold a new minimal OpenAPI spec with an example endpoint.
    Init(InitArgs),
    /// Convert an OpenAPI spec between versions 3.0 and 3.1.
    Convert(ConvertArgs),
    /// Generate a static HTML documentation site from an OpenAPI spec.
    Docs(DocsArgs),
    /// Generate SDK test files with mock servers from an OpenAPI spec.
    Test(TestArgs),
    /// List all API versions found in a spec directory or show a file's version.
    Versions(VersionsArgs),
    /// Merge multiple OpenAPI spec files into one.
    Merge(MergeArgs),
    /// Generate SDKs for all specs in a workspace config.
    Workspace(WorkspaceArgs),
    /// Generate a workspace config by scanning a directory for spec files.
    WorkspaceInit(WorkspaceInitArgs),
    /// Compare two spec versions and generate a migration guide.
    Migrate(MigrateArgs),
    /// Analyze a spec for redundancy, unused schemas, and size issues.
    Analyze(AnalyzeArgs),
    /// Infer an OpenAPI spec from a sample JSON request/response body.
    Infer(InferArgs),
    /// Verify a running API matches its OpenAPI spec by hitting endpoints.
    Verify(VerifyArgs),
    /// Track how a schema has evolved across git commits.
    Evolution(EvolutionArgs),
    /// Start a local mock HTTP server from a spec's example responses.
    Mock(MockArgs),
    /// Export an OpenAPI spec as a Swagger Editor-compatible bundle.
    Export(ExportArgs),
    /// Generate a working demo Petstore spec with realistic examples.
    Demo(DemoArgs),
    /// Generate a CHANGELOG.md from an OpenAPI spec.
    Changelog(ChangelogArgs),
    /// Browse, search, and manage the community spec marketplace.
    Market(MarketArgs),
    /// Apply automatic API versioning to an OpenAPI spec's endpoint paths.
    Version(VersionArgs),
    /// Profile the performance of API endpoints from an OpenAPI spec.
    Profile(ProfileArgs),
    /// Manage WASM emitter plugins.
    Plugin(PluginArgs),
}

#[derive(Args, Debug)]
struct GenerateArgs {
    /// Path to the OpenAPI spec (YAML or JSON).
    spec: PathBuf,

    /// Output directory for the generated SDK. Created if missing.
    #[arg(short, long, default_value = "./generated")]
    out: PathBuf,

    /// Target language.
    #[arg(short = 'l', long = "lang", value_enum, default_value_t = Lang::Ts)]
    lang: Lang,

    /// Package/module/crate name (language-specific default if omitted).
    #[arg(short = 'n', long = "name")]
    package_name: Option<String>,

    /// Generate for a specific API version (when spec is a directory).
    #[arg(long)]
    version: Option<String>,

    /// Output detailed timing for each pipeline stage.
    #[arg(long)]
    profile: bool,

    /// Include webhook handler types in the generated SDK (OpenAPI 3.1).
    #[arg(long)]
    include_webhooks: bool,

    /// Comma-separated list of locale codes for i18n error messages (e.g. "en,es,fr").
    /// When provided, the generated SDK includes localized error message files.
    #[arg(long, value_delimiter = ',')]
    locale: Option<Vec<String>>,

    /// Auto-generate CHANGELOG.md in the output directory.
    #[arg(long)]
    changelog: bool,

    /// Previous spec to diff against for changelog generation (used with --changelog).
    #[arg(long = "changelog-previous")]
    changelog_previous: Option<PathBuf>,

    /// Log verbosity.
    #[arg(short = 'v', long = "log-level", value_enum, default_value_t = LogLevel::Info)]
    log_level: LogLevel,

    /// Apply URL path versioning prefix before generation (e.g. "v1", "v2").
    #[arg(long)]
    version_prefix: Option<String>,

    /// Use a WASM plugin emitter by name (loaded from plugin marketplace or .specforge.yaml).
    #[arg(long = "plugin")]
    plugin: Option<String>,
}

#[derive(Args, Debug)]
struct CheckArgs {
    /// Path to the OpenAPI spec (YAML or JSON).
    spec: PathBuf,

    /// Treat warnings as errors.
    #[arg(long)]
    strict: bool,

    /// List all deprecated operations and schemas found in the spec.
    #[arg(long)]
    deprecations: bool,

    /// Disable a lint rule (can be repeated).
    #[arg(long = "disable", value_name = "RULE")]
    disable_rules: Vec<String>,

    /// Enable a lint rule (can be repeated).
    #[arg(long = "enable", value_name = "RULE")]
    enable_rules: Vec<String>,

    /// Set rule severity as RULE:SEVERITY where SEVERITY is error, warning, or off (can be repeated).
    #[arg(long = "severity", value_name = "RULE:SEVERITY")]
    severity_overrides: Vec<String>,

    /// Path to a lint config YAML file.
    #[arg(long = "config", value_name = "FILE")]
    config_file: Option<PathBuf>,

    /// Log verbosity.
    #[arg(short = 'v', long = "log-level", value_enum, default_value_t = LogLevel::Info)]
    log_level: LogLevel,
}

#[derive(Args, Debug)]
struct DiffArgs {
    /// Path to the old (baseline) OpenAPI spec.
    old: PathBuf,

    /// Path to the new OpenAPI spec.
    new: PathBuf,

    /// Show only breaking changes.
    #[arg(long)]
    breaking_only: bool,

    /// Log verbosity.
    #[arg(short = 'v', long = "log-level", value_enum, default_value_t = LogLevel::Info)]
    log_level: LogLevel,
}

#[derive(Args, Debug)]
struct EmitArgs {
    /// Path to the OpenAPI spec (YAML or JSON). Not required when --schema is used.
    spec: Option<PathBuf>,

    /// Print the IR JSON Schema and exit (for external tooling / validation).
    #[arg(long)]
    schema: bool,

    /// Output newline-delimited JSON (NDJSON) — one JSON object per line.
    /// Header line first, then one line per schema and operation.
    #[arg(long)]
    stream: bool,

    /// Output detailed timing for each pipeline stage.
    #[arg(long)]
    profile: bool,

    /// Log verbosity.
    #[arg(short = 'v', long = "log-level", value_enum, default_value_t = LogLevel::Warn)]
    log_level: LogLevel,
}

#[derive(Args, Debug)]
struct InitArgs {
    /// Output directory for the generated spec files.
    #[arg(short, long, default_value = ".")]
    out: PathBuf,

    /// API title.
    #[arg(long, default_value = "My API")]
    title: String,

    /// API version.
    #[arg(long, default_value = "1.0.0")]
    version: String,

    /// Default maximum retries for generated endpoints.
    #[arg(long)]
    retry_default: Option<u32>,

    /// HTTP methods that are retryable (can be repeated).
    #[arg(long = "retry-on", value_name = "METHOD")]
    retry_on: Option<Vec<String>>,

    /// Log verbosity.
    #[arg(short = 'v', long = "log-level", value_enum, default_value_t = LogLevel::Warn)]
    log_level: LogLevel,
}

#[derive(Args, Debug)]
struct DocsArgs {
    /// Path to the OpenAPI spec (YAML or JSON).
    spec: PathBuf,

    /// Output directory for the documentation site.
    #[arg(short, long, default_value = "./docs")]
    out: PathBuf,

    /// Log verbosity.
    #[arg(short = 'v', long = "log-level", value_enum, default_value_t = LogLevel::Info)]
    log_level: LogLevel,
}

#[derive(Args, Debug)]
struct TestArgs {
    /// Path to the OpenAPI spec (YAML or JSON).
    spec: PathBuf,

    /// Output directory for the generated test files.
    #[arg(short, long, default_value = "./tests")]
    out: PathBuf,

    /// Target language for test generation.
    #[arg(short = 'l', long = "lang", value_enum, default_value_t = Lang::Ts)]
    lang: Lang,

    /// Log verbosity.
    #[arg(short = 'v', long = "log-level", value_enum, default_value_t = LogLevel::Info)]
    log_level: LogLevel,
}

#[derive(Args, Debug)]
struct ConvertArgs {
    /// Path to the OpenAPI spec (YAML or JSON).
    spec: PathBuf,

    /// Target OpenAPI version.
    #[arg(long, value_enum, default_value_t = OpenApiVersion::V31)]
    to: OpenApiVersion,

    /// Output file (default: stdout).
    #[arg(short, long)]
    out: Option<PathBuf>,

    /// Log verbosity.
    #[arg(short = 'v', long = "log-level", value_enum, default_value_t = LogLevel::Warn)]
    log_level: LogLevel,
}

#[derive(Args, Debug)]
struct VersionsArgs {
    /// Path to a spec file or directory containing versioned specs.
    spec: PathBuf,

    /// Log verbosity.
    #[arg(short = 'v', long = "log-level", value_enum, default_value_t = LogLevel::Info)]
    log_level: LogLevel,
}

#[derive(Args, Debug)]
struct MergeArgs {
    /// Paths to OpenAPI spec files to merge (can specify multiple).
    #[arg(required = true)]
    specs: Vec<PathBuf>,

    /// Output file (default: stdout).
    #[arg(short, long)]
    out: Option<PathBuf>,

    /// Output format: json or yaml.
    #[arg(long, default_value = "yaml")]
    format: String,

    #[arg(short = 'v', long, value_enum, default_value_t = LogLevel::Info)]
    log_level: LogLevel,
}

#[derive(Args, Debug)]
struct WorkspaceArgs {
    /// Path to workspace config file (default: .specforge-workspace.yaml).
    #[arg(short, long, default_value = ".specforge-workspace.yaml")]
    config: PathBuf,

    /// Only generate for a specific spec name.
    #[arg(long)]
    only: Option<String>,

    /// Dry run: show what would be generated without writing.
    #[arg(long)]
    dry_run: bool,

    /// Log verbosity.
    #[arg(short = 'v', long = "log-level", value_enum, default_value_t = LogLevel::Info)]
    log_level: LogLevel,
}

#[derive(Args, Debug)]
struct WorkspaceInitArgs {
    /// Directory to scan for spec files.
    #[arg(default_value = ".")]
    dir: PathBuf,

    /// Output config file.
    #[arg(short, long, default_value = ".specforge-workspace.yaml")]
    out: PathBuf,

    /// Log verbosity.
    #[arg(short = 'v', long = "log-level", value_enum, default_value_t = LogLevel::Info)]
    log_level: LogLevel,
}

#[derive(Args, Debug)]
struct MigrateArgs {
    /// Path to the old (baseline) OpenAPI spec.
    old: PathBuf,

    /// Path to the new OpenAPI spec.
    new: PathBuf,

    /// Output file (default: stdout).
    #[arg(short, long)]
    out: Option<PathBuf>,

    /// Log verbosity.
    #[arg(short = 'v', long = "log-level", value_enum, default_value_t = LogLevel::Info)]
    log_level: LogLevel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum AnalyzeFormat {
    Text,
    Json,
    Markdown,
}
#[derive(Args, Debug)]
struct AnalyzeArgs {
    spec: PathBuf,
    #[arg(long, value_enum, default_value_t = AnalyzeFormat::Text)]
    format: AnalyzeFormat,
    #[arg(short = 'v', long = "log-level", value_enum, default_value_t = LogLevel::Info)]
    log_level: LogLevel,
}

#[derive(Args, Debug)]
struct InferArgs {
    /// Path to a JSON file, or "-" to read from stdin.
    input: String,

    /// Schema / model name (default: "Inferred").
    #[arg(long, default_value = "Inferred")]
    name: String,

    /// Output file (default: stdout).
    #[arg(short, long)]
    out: Option<PathBuf>,

    /// API title for the generated spec.
    #[arg(long, default_value = "Inferred API")]
    title: String,

    /// API version for the generated spec.
    #[arg(long, default_value = "1.0.0")]
    version: String,

    /// Log verbosity.
    #[arg(short = 'v', long = "log-level", value_enum, default_value_t = LogLevel::Info)]
    log_level: LogLevel,
}

#[derive(Args, Debug)]
struct VerifyArgs {
    /// Path to the OpenAPI spec (YAML or JSON).
    spec: PathBuf,

    /// Base URL of the running API to verify against.
    #[arg(long)]
    base_url: String,

    /// Authorization header value (e.g. "Bearer <token>").
    #[arg(long)]
    auth: Option<String>,

    /// Per-request timeout in milliseconds (default: 5000).
    #[arg(long, default_value_t = 5000)]
    timeout: u64,

    /// Log verbosity.
    #[arg(short = 'v', long = "log-level", value_enum, default_value_t = LogLevel::Info)]
    log_level: LogLevel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum EvolutionFormat {
    Text,
    Json,
    Markdown,
}

#[derive(Args, Debug)]
struct EvolutionArgs {
    /// Path to the OpenAPI spec (YAML or JSON).
    spec: PathBuf,

    /// Output format.
    #[arg(long, value_enum, default_value_t = EvolutionFormat::Text)]
    format: EvolutionFormat,

    /// Maximum number of versions to show (most recent first).
    #[arg(long, default_value_t = 10)]
    limit: usize,

    /// Log verbosity.
    #[arg(short = 'v', long = "log-level", value_enum, default_value_t = LogLevel::Info)]
    log_level: LogLevel,
}

#[derive(Args, Debug)]
struct MockArgs {
    /// Path to the OpenAPI spec (YAML or JSON).
    spec: PathBuf,

    /// Port to listen on (default: random available port).
    #[arg(long)]
    port: Option<u16>,

    /// Host to bind to (default: 127.0.0.1).
    #[arg(long, default_value = "127.0.0.1")]
    host: String,

    /// Log verbosity.
    #[arg(short = 'v', long = "log-level", value_enum, default_value_t = LogLevel::Info)]
    log_level: LogLevel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum ExportFormat {
    /// Swagger Editor compatible bundle (all refs inlined).
    #[value(name = "swagger-editor")]
    SwaggerEditor,
}

#[derive(Args, Debug)]
struct ExportArgs {
    /// Path to the OpenAPI spec (YAML or JSON).
    spec: PathBuf,

    /// Export format.
    #[arg(long, value_enum, default_value_t = ExportFormat::SwaggerEditor)]
    format: ExportFormat,

    /// Output file (default: stdout).
    #[arg(short, long)]
    out: Option<PathBuf>,

    /// Log verbosity.
    #[arg(short = 'v', long = "log-level", value_enum, default_value_t = LogLevel::Info)]
    log_level: LogLevel,
}

#[derive(Args, Debug)]
struct DemoArgs {
    /// Output file (default: stdout).
    #[arg(short, long)]
    out: Option<PathBuf>,

    /// Log verbosity.
    #[arg(short = 'v', long = "log-level", value_enum, default_value_t = LogLevel::Info)]
    log_level: LogLevel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum ChangelogOutputFormat {
    /// Markdown output (default).
    Markdown,
    /// JSON output.
    Json,
}

#[derive(Args, Debug)]
struct ChangelogArgs {
    /// Path to the OpenAPI spec (YAML or JSON).
    spec: PathBuf,

    /// Override version (default: from spec info.version).
    #[arg(long)]
    version: Option<String>,

    /// Previous spec for diff.
    #[arg(long)]
    previous: Option<PathBuf>,

    /// Output file (default: CHANGELOG.md in current directory).
    #[arg(short, long)]
    out: Option<PathBuf>,

    /// Suggest semantic version bump (major/minor/patch) based on changes.
    #[arg(long)]
    suggest_version: bool,

    /// Output format.
    #[arg(long, value_enum, default_value_t = ChangelogOutputFormat::Markdown)]
    format: ChangelogOutputFormat,

    /// Log verbosity.
    #[arg(short = 'v', long = "log-level", value_enum, default_value_t = LogLevel::Info)]
    log_level: LogLevel,
}

/// Browse, search, and manage the community spec marketplace.
#[derive(Args, Debug)]
struct MarketArgs {
    #[command(subcommand)]
    market_cmd: MarketCommands,

    /// Path to an extra marketplace index JSON to merge.
    #[arg(long = "index")]
    extra_index: Option<PathBuf>,
}

#[derive(Subcommand, Debug)]
enum MarketCommands {
    /// Search the marketplace by keyword.
    Search(MarketSearchArgs),
    /// List all available specs.
    List(MarketListArgs),
    /// Show detailed info about a specific spec.
    Info(MarketInfoArgs),
    /// Add a local spec to the marketplace index.
    Add(MarketAddArgs),
}

#[derive(Args, Debug)]
struct MarketSearchArgs {
    /// Search query (matches name, description, tags, author).
    query: String,

    /// Output format.
    #[arg(long, default_value = "text")]
    format: String,

    /// Log verbosity.
    #[arg(short = 'v', long = "log-level", value_enum, default_value_t = LogLevel::Info)]
    log_level: LogLevel,
}

#[derive(Args, Debug)]
struct MarketListArgs {
    /// Output format (text or json).
    #[arg(long, default_value = "text")]
    format: String,

    /// Filter by tag.
    #[arg(long)]
    tag: Option<String>,

    /// Log verbosity.
    #[arg(short = 'v', long = "log-level", value_enum, default_value_t = LogLevel::Info)]
    log_level: LogLevel,
}

#[derive(Args, Debug)]
struct MarketInfoArgs {
    /// Name of the spec to inspect.
    name: String,

    /// Log verbosity.
    #[arg(short = 'v', long = "log-level", value_enum, default_value_t = LogLevel::Info)]
    log_level: LogLevel,
}

#[derive(Args, Debug)]
struct MarketAddArgs {
    /// Path to a local OpenAPI spec file.
    path: PathBuf,

    /// Output marketplace index file (default: marketplace.json in current dir).
    #[arg(short, long, default_value = "marketplace.json")]
    out: PathBuf,

    /// Log verbosity.
    #[arg(short = 'v', long = "log-level", value_enum, default_value_t = LogLevel::Info)]
    log_level: LogLevel,
}

// ---------------------------------------------------------------------------
// Plugin subcommand args
// ---------------------------------------------------------------------------

/// Manage WASM emitter plugins.
#[derive(Args, Debug)]
struct PluginArgs {
    #[command(subcommand)]
    plugin_cmd: PluginCommands,

    /// Path to an extra plugin index JSON to merge.
    #[arg(long = "index")]
    extra_index: Option<PathBuf>,
}

#[derive(Subcommand, Debug)]
enum PluginCommands {
    /// List all available plugins.
    List(PluginListArgs),
    /// Search plugins by keyword.
    Search(PluginSearchArgs),
    /// Show detailed info about a specific plugin.
    Info(PluginInfoArgs),
    /// Download and install a plugin.
    Install(PluginInstallArgs),
}

#[derive(Args, Debug)]
struct PluginListArgs {
    /// Output format (text or json).
    #[arg(long, default_value = "text")]
    format: String,

    /// Filter by target language.
    #[arg(long)]
    language: Option<String>,

    /// Log verbosity.
    #[arg(short = 'v', long = "log-level", value_enum, default_value_t = LogLevel::Info)]
    log_level: LogLevel,
}

#[derive(Args, Debug)]
struct PluginSearchArgs {
    /// Search query (matches name, description, language, author).
    query: String,

    /// Output format.
    #[arg(long, default_value = "text")]
    format: String,

    /// Log verbosity.
    #[arg(short = 'v', long = "log-level", value_enum, default_value_t = LogLevel::Info)]
    log_level: LogLevel,
}

#[derive(Args, Debug)]
struct PluginInfoArgs {
    /// Name of the plugin to inspect.
    name: String,

    /// Log verbosity.
    #[arg(short = 'v', long = "log-level", value_enum, default_value_t = LogLevel::Info)]
    log_level: LogLevel,
}

#[derive(Args, Debug)]
struct PluginInstallArgs {
    /// Name of the plugin to install.
    name: String,

    /// Directory to install the plugin WASM file into (default: ./plugins).
    #[arg(short, long, default_value = "./plugins")]
    dir: PathBuf,

    /// Log verbosity.
    #[arg(short = 'v', long = "log-level", value_enum, default_value_t = LogLevel::Info)]
    log_level: LogLevel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum ProfileFormat {
    Text,
    Json,
    Markdown,
}

/// Profile the performance of API endpoints declared in an OpenAPI spec.
#[derive(Args, Debug)]
struct ProfileArgs {
    /// Path to the OpenAPI spec (YAML or JSON).
    spec: PathBuf,

    /// Base URL of the running API to profile against.
    #[arg(long)]
    base_url: String,

    /// Profile a specific endpoint (substring match on path).
    #[arg(long)]
    endpoint: Option<String>,

    /// Number of requests per endpoint.
    #[arg(long, default_value_t = 100)]
    requests: usize,

    /// Number of concurrent requests (reserved for future use).
    #[arg(long, default_value_t = 10)]
    concurrency: usize,

    /// Per-request timeout in milliseconds.
    #[arg(long, default_value_t = 5000)]
    timeout: u64,

    /// Authorization header value (e.g. "Bearer <token>").
    #[arg(long)]
    auth: Option<String>,

    /// Output format.
    #[arg(long, value_enum, default_value_t = ProfileFormat::Text)]
    format: ProfileFormat,

    /// Log verbosity.
    #[arg(short = 'v', long = "log-level", value_enum, default_value_t = LogLevel::Info)]
    log_level: LogLevel,
}

/// Versioning strategy for the `version` subcommand.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum VersionStrategyArg {
    /// URL path prefix (/v1/pets)
    Url,
    /// Header (API-Version: 1)
    Header,
    /// Query parameter (?version=1)
    Query,
    /// None (no versioning)
    None,
}

/// Apply automatic API versioning to endpoint paths in an OpenAPI spec.
#[derive(Args, Debug)]
struct VersionArgs {
    /// Path to the OpenAPI spec (YAML or JSON).
    spec: PathBuf,

    /// Versioning strategy.
    #[arg(long, value_enum, default_value_t = VersionStrategyArg::Url)]
    strategy: VersionStrategyArg,

    /// URL prefix for path strategy (default: v1).
    #[arg(long, default_value = "v1")]
    prefix: String,

    /// Header name for header strategy (default: API-Version).
    #[arg(long)]
    header: Option<String>,

    /// Output file (default: stdout).
    #[arg(short, long)]
    out: Option<PathBuf>,

    /// Log verbosity.
    #[arg(short = 'v', long = "log-level", value_enum, default_value_t = LogLevel::Info)]
    log_level: LogLevel,
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    let level = match &cli.command {
        Commands::Generate(args) => args.log_level,
        Commands::Check(args) => args.log_level,
        Commands::Diff(args) => args.log_level,
        Commands::Emit(args) => args.log_level,
        Commands::Init(args) => args.log_level,
        Commands::Convert(args) => args.log_level,
        Commands::Docs(args) => args.log_level,
        Commands::Test(args) => args.log_level,
        Commands::Versions(args) => args.log_level,
        Commands::Merge(args) => args.log_level,
        Commands::Workspace(args) => args.log_level,
        Commands::WorkspaceInit(args) => args.log_level,
        Commands::Migrate(args) => args.log_level,
        Commands::Analyze(args) => args.log_level,
        Commands::Infer(args) => args.log_level,
        Commands::Verify(args) => args.log_level,
        Commands::Evolution(args) => args.log_level,
        Commands::Mock(args) => args.log_level,
        Commands::Export(args) => args.log_level,
        Commands::Demo(args) => args.log_level,
        Commands::Changelog(args) => args.log_level,
        Commands::Market(args) => match &args.market_cmd {
            MarketCommands::Search(a) => a.log_level,
            MarketCommands::List(a) => a.log_level,
            MarketCommands::Info(a) => a.log_level,
            MarketCommands::Add(a) => a.log_level,
        },
        Commands::Version(args) => args.log_level,
        Commands::Profile(args) => args.log_level,
        Commands::Plugin(args) => match &args.plugin_cmd {
            PluginCommands::List(a) => a.log_level,
            PluginCommands::Search(a) => a.log_level,
            PluginCommands::Info(a) => a.log_level,
            PluginCommands::Install(a) => a.log_level,
        },
    };

    let level_str = match level {
        LogLevel::Error => "error",
        LogLevel::Warn => "warn",
        LogLevel::Info => "info",
        LogLevel::Debug => "debug",
        LogLevel::Trace => "trace",
    };
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(level_str)),
        )
        .with_target(false)
        .init();

    match cli.command {
        Commands::Generate(args) => match run_generate(args) {
            Ok(count) => {
                info!("generated {count} files");
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("error: {e:#}");
                warn!("{e:#}");
                ExitCode::FAILURE
            }
        },
        Commands::Check(args) => match run_check(&args) {
            Ok(has_errors) => {
                if has_errors {
                    ExitCode::FAILURE
                } else {
                    ExitCode::SUCCESS
                }
            }
            Err(e) => {
                eprintln!("error: {e:#}");
                warn!("{e:#}");
                ExitCode::FAILURE
            }
        },
        Commands::Diff(args) => match run_diff(&args) {
            Ok(has_breaking) => {
                if has_breaking {
                    ExitCode::FAILURE
                } else {
                    ExitCode::SUCCESS
                }
            }
            Err(e) => {
                eprintln!("error: {e:#}");
                warn!("{e:#}");
                ExitCode::FAILURE
            }
        },
        Commands::Emit(args) => match run_emit(&args) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("error: {e:#}");
                warn!("{e:#}");
                ExitCode::FAILURE
            }
        },
        Commands::Init(args) => match run_init(&args) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("error: {e:#}");
                warn!("{e:#}");
                ExitCode::FAILURE
            }
        },
        Commands::Convert(args) => match run_convert(&args) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("error: {e:#}");
                warn!("{e:#}");
                ExitCode::FAILURE
            }
        },
        Commands::Docs(args) => match run_docs(&args) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("error: {e:#}");
                warn!("{e:#}");
                ExitCode::FAILURE
            }
        },
        Commands::Test(args) => match run_test(&args) {
            Ok(count) => {
                info!("generated {count} test file(s)");
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("error: {e:#}");
                warn!("{e:#}");
                ExitCode::FAILURE
            }
        },
        Commands::Versions(args) => match run_versions(args) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("error: {e:#}");
                warn!("{e:#}");
                ExitCode::FAILURE
            }
        },
        Commands::Merge(args) => match run_merge(&args) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("error: {e:#}");
                warn!("{e:#}");
                ExitCode::FAILURE
            }
        },
        Commands::Workspace(args) => match run_workspace(&args) {
            Ok(result) => {
                if args.dry_run {
                    info!("dry run complete");
                } else {
                    info!(
                        "workspace: {} spec(s), {} output(s), {} file(s)",
                        result.specs_processed, result.outputs_generated, result.files_written
                    );
                }
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("error: {e:#}");
                warn!("{e:#}");
                ExitCode::FAILURE
            }
        },
        Commands::WorkspaceInit(args) => match run_workspace_init(&args) {
            Ok(result) => {
                eprintln!(
                    "Created {} with {} spec(s)",
                    result.config_path.display(),
                    result.specs_found
                );
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("error: {e:#}");
                warn!("{e:#}");
                ExitCode::FAILURE
            }
        },
        Commands::Migrate(args) => match run_migrate(&args) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("error: {e:#}");
                warn!("{e:#}");
                ExitCode::FAILURE
            }
        },
        Commands::Analyze(args) => match run_analyze(&args) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("error: {e:#}");
                warn!("{e:#}");
                ExitCode::FAILURE
            }
        },
        Commands::Infer(args) => match run_infer(&args) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("error: {e:#}");
                warn!("{e:#}");
                ExitCode::FAILURE
            }
        },
        Commands::Verify(args) => match run_verify(&args) {
            Ok(results) => {
                let failures = results.iter().filter(|r| !r.issues.is_empty()).count();
                if failures > 0 {
                    ExitCode::FAILURE
                } else {
                    ExitCode::SUCCESS
                }
            }
            Err(e) => {
                eprintln!("error: {e:#}");
                warn!("{e:#}");
                ExitCode::FAILURE
            }
        },
        Commands::Evolution(args) => match run_evolution(&args) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("error: {e:#}");
                warn!("{e:#}");
                ExitCode::FAILURE
            }
        },
        Commands::Mock(args) => match run_mock(&args) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("error: {e:#}");
                warn!("{e:#}");
                ExitCode::FAILURE
            }
        },
        Commands::Export(args) => match run_export(&args) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("error: {e:#}");
                warn!("{e:#}");
                ExitCode::FAILURE
            }
        },
        Commands::Demo(args) => match run_demo(&args) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("error: {e:#}");
                warn!("{e:#}");
                ExitCode::FAILURE
            }
        },
        Commands::Changelog(args) => match run_changelog(&args) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("error: {e:#}");
                warn!("{e:#}");
                ExitCode::FAILURE
            }
        },
        Commands::Market(args) => match run_market(&args) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("error: {e:#}");
                warn!("{e:#}");
                ExitCode::FAILURE
            }
        },
        Commands::Version(args) => match run_version(&args) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("error: {e:#}");
                warn!("{e:#}");
                ExitCode::FAILURE
            }
        },
        Commands::Profile(args) => match run_profile(&args) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("error: {e:#}");
                warn!("{e:#}");
                ExitCode::FAILURE
            }
        },
        Commands::Plugin(args) => match run_plugin(&args) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("error: {e:#}");
                warn!("{e:#}");
                ExitCode::FAILURE
            }
        },
    }
}

fn run_generate(cli: GenerateArgs) -> Result<usize> {
    let start = std::time::Instant::now();

    // Resolve the spec path: if it's a directory, use --version to find the right file.
    let spec_path = resolve_spec_path(&cli.spec, cli.version.as_deref())
        .context("failed to resolve spec path")?;

    info!("reading spec: {}", spec_path.display());
    let t0 = std::time::Instant::now();

    let doc = if cli.include_webhooks {
        // Use full parsing that preserves webhooks (OpenAPI 3.1).
        let parsed = specforge_core::parse_file_full(&spec_path)
            .with_context(|| format!("failed to parse spec at {}", spec_path.display()))?;
        let parse_time = t0.elapsed();

        info!("resolving document into IR (with webhooks)");
        let t1 = std::time::Instant::now();
        let doc = specforge_core::resolve_with_webhooks(&parsed.spec, parsed.webhooks.as_ref())
            .context("failed to resolve spec into IR")?;
        let resolve_time = t1.elapsed();

        info!(
            "resolved: {} schemas, {} operations, {} webhook(s), {} security scheme(s)",
            doc.schemas.models.len(),
            doc.operations.len(),
            doc.webhooks.len(),
            doc.security.len(),
        );

        if cli.profile {
            eprintln!("Profile:");
            eprintln!("  parse:   {parse_time:?}");
            eprintln!("  resolve: {resolve_time:?}");
        }

        doc
    } else {
        let spec = parse_file(&spec_path)
            .with_context(|| format!("failed to parse spec at {}", spec_path.display()))?;
        let parse_time = t0.elapsed();

        info!("resolving document into IR");
        let t1 = std::time::Instant::now();
        let doc = resolve(&spec).context("failed to resolve spec into IR")?;
        let resolve_time = t1.elapsed();

        info!(
            "resolved: {} schemas, {} operations, {} security scheme(s)",
            doc.schemas.models.len(),
            doc.operations.len(),
            doc.security.len(),
        );

        if cli.profile {
            eprintln!("Profile:");
            eprintln!("  parse:   {parse_time:?}");
            eprintln!("  resolve: {resolve_time:?}");
        }

        doc
    };

    // Apply versioning if --version-prefix is set.
    let mut doc = doc;
    if let Some(ref prefix) = cli.version_prefix {
        info!("applying URL path versioning with prefix: {prefix}");
        let versioning_config = VersioningConfig {
            strategy: VersionStrategy::UrlPath,
            prefix: Some(prefix.clone()),
            header_name: None,
        };
        apply_versioning(&mut doc, &versioning_config);
        for op in &doc.operations {
            info!("  {} {}", op.method.upper(), op.path);
        }
    }

    // Resolve --plugin flag: try .specforge.yaml first, then the plugin marketplace.
    if let Some(ref plugin_name) = cli.plugin {
        let plugin_path = resolve_plugin_path(plugin_name)
            .with_context(|| format!("failed to resolve plugin '{plugin_name}'"))?;
        info!(
            "using WASM plugin: {} ({})",
            plugin_name,
            plugin_path.display()
        );
        // NOTE: Actual WASM runtime execution requires a WASM engine (e.g.
        // wasmtime).  For now we record the resolved path so downstream tooling
        // can pick it up.  The IR is still emitted through the built-in emitters
        // and the plugin path is written alongside for reference.
        std::fs::create_dir_all(&cli.out)
            .with_context(|| format!("failed to create output directory {}", cli.out.display()))?;
        let marker = cli.out.join(".plugin-used");
        std::fs::write(&marker, plugin_path.display().to_string())
            .with_context(|| format!("failed to write {}", marker.display()))?;
    }

    info!("emitting {:?} SDK to: {}", cli.lang, cli.out.display());

    let t2 = std::time::Instant::now();

    // Build i18n config if locales were specified.
    let i18n_config = cli.locale.as_ref().map(|locales| {
        info!("generating i18n files for locales: {}", locales.join(", "));
        specforge_core::I18nConfig::from_locales(locales)
    });

    let written = match cli.lang {
        Lang::Ts => {
            let opts = specforge_ts::GeneratorOptions {
                out_dir: cli.out.clone(),
                package_name: cli.package_name.clone(),
                i18n: i18n_config.clone(),
            };
            specforge_ts::generate(&doc, &opts).context("failed to emit TypeScript SDK")?
        }
        Lang::Go => {
            let opts = specforge_go::GeneratorOptions {
                out_dir: cli.out.clone(),
                module_path: cli.package_name.clone(),
                package_name: None,
                i18n: i18n_config.clone(),
            };
            specforge_go::generate(&doc, &opts).context("failed to emit Go SDK")?
        }
        Lang::Rust => {
            let opts = specforge_rust::GeneratorOptions {
                out_dir: cli.out.clone(),
                crate_name: cli.package_name.clone(),
                i18n: i18n_config.clone(),
            };
            specforge_rust::generate(&doc, &opts).context("failed to emit Rust SDK")?
        }
    };
    let emit_time = t2.elapsed();

    let mut file_count = written.len();

    // Auto-generate CHANGELOG.md if --changelog is set.
    if cli.changelog {
        let changelog_opts = ChangelogOptions {
            version: cli.version.clone(),
            previous_spec: cli
                .changelog_previous
                .as_ref()
                .map(|p| p.display().to_string()),
            suggest_version: false,
            format: Default::default(),
        };
        let changelog_content = generate_changelog(&doc, &changelog_opts);
        let changelog_path = cli.out.join("CHANGELOG.md");
        std::fs::write(&changelog_path, &changelog_content)
            .with_context(|| format!("failed to write {}", changelog_path.display()))?;
        info!("wrote {}", changelog_path.display());
        file_count += 1;
    }

    if file_count == 0 {
        bail!("emitter wrote zero files");
    }

    if cli.profile {
        eprintln!("Profile:");
        eprintln!("  emit:    {emit_time:?}");
        eprintln!("  total:   {:?}", start.elapsed());
    }

    Ok(file_count)
}

fn run_check(cli: &CheckArgs) -> Result<bool> {
    info!("reading spec: {}", cli.spec.display());
    let spec = parse_file(&cli.spec)
        .with_context(|| format!("failed to parse spec at {}", cli.spec.display()))?;

    info!("resolving document into IR");
    let doc = resolve(&spec).context("failed to resolve spec into IR")?;

    info!(
        "resolved: {} schemas, {} operations",
        doc.schemas.models.len(),
        doc.operations.len(),
    );

    // Build lint config: start from file or defaults, then apply CLI overrides.
    let mut lint_cfg = match &cli.config_file {
        Some(path) => {
            info!("loading lint config from {}", path.display());
            LintConfig::load_from_file(path)
        }
        None => lint_config::LintConfig::load(),
    };

    for rule in &cli.disable_rules {
        lint_cfg.set_enabled(rule, false);
    }
    for rule in &cli.enable_rules {
        lint_cfg.set_enabled(rule, true);
    }
    for override_str in &cli.severity_overrides {
        let (rule, sev) = parse_severity_override(override_str)
            .with_context(|| format!("invalid --severity value: {override_str:?}"))?;
        lint_cfg.set_severity(&rule, sev);
    }

    let diagnostics = lint::lint_with_config(&doc, &lint_cfg);

    let mut has_errors = false;
    for diag in &diagnostics {
        let is_error =
            diag.severity == Severity::Error || (cli.strict && diag.severity == Severity::Warning);
        if is_error {
            eprintln!("error: {diag}");
            has_errors = true;
        } else {
            eprintln!("warning: {diag}");
        }
    }

    if diagnostics.is_empty() {
        info!("no issues found");
    } else {
        let errors = diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .count();
        let warnings = diagnostics.len() - errors;
        if errors > 0 {
            eprintln!("\n{errors} error(s), {warnings} warning(s)",);
        } else if cli.strict && warnings > 0 {
            eprintln!("\n{warnings} warning(s) treated as errors (--strict)",);
            has_errors = true;
        } else {
            eprintln!("\n{warnings} warning(s)",);
        }
    }

    // Deprecation scan.
    if cli.deprecations {
        let deprecations = specforge_core::find_deprecations(&doc);
        if deprecations.is_empty() {
            eprintln!("\nno deprecations found");
        } else {
            eprintln!("\nDeprecations ({}):\n", deprecations.len());
            for dep in &deprecations {
                eprintln!("  {dep}");
            }
        }
    }

    Ok(has_errors)
}

/// Parse a `RULE:SEVERITY` string into a (rule_name, RuleSeverity) pair.
fn parse_severity_override(s: &str) -> Result<(String, RuleSeverity)> {
    let (rule, sev) = s
        .rsplit_once(':')
        .ok_or_else(|| anyhow::anyhow!("expected RULE:SEVERITY, got {s:?}"))?;
    let sev = match sev.to_lowercase().as_str() {
        "error" => RuleSeverity::Error,
        "warning" | "warn" => RuleSeverity::Warning,
        "off" | "none" => RuleSeverity::Off,
        other => bail!("unknown severity {other:?}, expected error/warning/off"),
    };
    Ok((rule.to_string(), sev))
}

fn run_diff(cli: &DiffArgs) -> Result<bool> {
    info!("reading old spec: {}", cli.old.display());
    let old_spec = parse_file(&cli.old)
        .with_context(|| format!("failed to parse old spec at {}", cli.old.display()))?;
    let old_doc = resolve(&old_spec).context("failed to resolve old spec")?;

    info!("reading new spec: {}", cli.new.display());
    let new_spec = parse_file(&cli.new)
        .with_context(|| format!("failed to parse new spec at {}", cli.new.display()))?;
    let new_doc = resolve(&new_spec).context("failed to resolve new spec")?;

    let findings = diff::diff(&old_doc, &new_doc);

    let mut has_breaking = false;
    for finding in &findings {
        if cli.breaking_only && finding.severity != DiffSeverity::Breaking {
            continue;
        }
        match finding.severity {
            DiffSeverity::Breaking => {
                eprintln!("breaking: {finding}");
                has_breaking = true;
            }
            DiffSeverity::Info => {
                eprintln!("info: {finding}");
            }
        }
    }

    let breaking_count = findings
        .iter()
        .filter(|f| f.severity == DiffSeverity::Breaking)
        .count();
    let info_count = findings.len() - breaking_count;

    if findings.is_empty() {
        info!("no differences found");
    } else {
        eprintln!("\n{breaking_count} breaking change(s), {info_count} info finding(s)");
    }

    Ok(has_breaking)
}

fn run_emit(cli: &EmitArgs) -> Result<()> {
    // --schema: print the IR JSON Schema and exit.
    if cli.schema {
        let schema = include_str!("../../../assets/ir-schema.json");
        println!("{schema}");
        return Ok(());
    }

    let start = std::time::Instant::now();

    let spec_path = cli
        .spec
        .as_ref()
        .context("a spec file is required (or use --schema)")?;
    info!("reading spec: {}", spec_path.display());
    let t0 = std::time::Instant::now();
    let spec = parse_file(spec_path)
        .with_context(|| format!("failed to parse spec at {}", spec_path.display()))?;
    let parse_time = t0.elapsed();

    info!("resolving document into IR");
    let t1 = std::time::Instant::now();
    let doc = resolve(&spec).context("failed to resolve spec into IR")?;
    let resolve_time = t1.elapsed();

    let t2 = std::time::Instant::now();
    if cli.stream {
        // NDJSON streaming mode: header, then one line per schema, then one per operation.
        let header = serde_json::json!({
            "_type": "header",
            "title": doc.title,
            "version": doc.version,
            "base_url": doc.base_url,
        });
        println!("{}", serde_json::to_string(&header)?);

        for (name, model) in doc.schemas.iter() {
            let line = serde_json::json!({
                "_type": "schema",
                "name": name,
                "model": model,
            });
            println!("{}", serde_json::to_string(&line)?);
        }

        for op in &doc.operations {
            let line = serde_json::json!({
                "_type": "operation",
                "operation_id": op.operation_id,
                "method": op.method,
                "path": op.path,
                "tag": op.tag,
                "summary": op.summary,
                "description": op.description,
                "parameters": op.parameters,
                "request_body": op.request_body,
                "responses": op.responses,
            });
            println!("{}", serde_json::to_string(&line)?);
        }
    } else {
        let json = serde_json::to_string_pretty(&doc).context("failed to serialize IR to JSON")?;
        println!("{json}");
    }
    let serialize_time = t2.elapsed();

    if cli.profile {
        eprintln!("Profile:");
        eprintln!("  parse:     {parse_time:?}");
        eprintln!("  resolve:   {resolve_time:?}");
        eprintln!("  serialize: {serialize_time:?}");
        eprintln!("  total:     {:?}", start.elapsed());
    }

    Ok(())
}

fn build_retry_yaml_block(max_retries: Option<u32>, retry_on: Option<&[String]>) -> String {
    if max_retries.is_none() && retry_on.is_none() {
        return String::new();
    }
    let mut lines = vec!["      x-retry:".to_string()];
    if let Some(max) = max_retries {
        lines.push(format!("        maxRetries: {max}"));
    }
    if let Some(methods) = retry_on {
        let retryable = methods.iter().any(|m| m.eq_ignore_ascii_case("GET"));
        lines.push(format!("        retryable: {retryable}"));
    }
    lines.join(
        "
",
    )
}

fn run_init(cli: &InitArgs) -> Result<()> {
    std::fs::create_dir_all(&cli.out)
        .with_context(|| format!("failed to create output directory {}", cli.out.display()))?;

    let title_escaped = cli.title.replace('"', "\\\"");
    let dollar_ref = ["$", "ref"].concat();
    let schema_path = ["#", "/", "components", "/schemas", "/HealthResponse"].concat();
    let retry_block = build_retry_yaml_block(cli.retry_default, cli.retry_on.as_deref());
    let openapi_yaml = vec![
        "openapi: \"3.0.3\"",
        "info:",
        &format!("  title: \"{}\"", title_escaped),
        &format!("  version: \"{}\"", cli.version),
        "  description: \"A minimal OpenAPI spec generated by specforge.\"",
        "servers:",
        "  - url: http://localhost:3000",
        "    description: Local development server",
        "paths:",
        "  /health:",
        "    get:",
        "      operationId: getHealth",
        "      summary: Health check",
        "      description: Returns the health status of the API.",
        "      tags:",
        "        - system",
        &retry_block,
        "      responses:",
        "        \"200\":",
        "          description: Service is healthy",
        "          content:",
        "            application/json:",
        "              schema:",
        &format!("                {}: \"{}\"", dollar_ref, schema_path),
        "components:",
        "  schemas:",
        "    HealthResponse:",
        "      type: object",
        "      required:",
        "        - status",
        "      properties:",
        "        status:",
        "          type: string",
        "          description: Current health status",
        "          example: \"ok\"",
        "",
    ]
    .join("\n");

    let readme = format!(
        r#"# {title}

This is a minimal OpenAPI spec scaffolded by [specforge](https://github.com/amafjarkasi/specforge-openapi-sdk-codegen).

## Generate an SDK

```bash
# TypeScript
specforge generate openapi.yaml -o ./sdk-ts -l ts

# Go
specforge generate openapi.yaml -o ./sdk-go -l go

# Rust
specforge generate openapi.yaml -o ./sdk-rust -l rust
```

## Lint the spec

```bash
specforge check openapi.yaml
```

## Emit the IR (for external tools)

```bash
specforge emit openapi.yaml
```
"#,
        title = cli.title,
    );

    let yaml_path = cli.out.join("openapi.yaml");
    std::fs::write(&yaml_path, openapi_yaml)
        .with_context(|| format!("failed to write {}", yaml_path.display()))?;

    let readme_path = cli.out.join("README.md");
    std::fs::write(&readme_path, readme)
        .with_context(|| format!("failed to write {}", readme_path.display()))?;

    eprintln!("Created {}", yaml_path.display());
    eprintln!("Created {}", readme_path.display());
    Ok(())
}

fn run_docs(cli: &DocsArgs) -> Result<()> {
    info!("reading spec: {}", cli.spec.display());
    let spec = parse_file(&cli.spec)
        .with_context(|| format!("failed to parse spec at {}", cli.spec.display()))?;

    info!("resolving document into IR");
    let doc = resolve(&spec).context("failed to resolve spec into IR")?;

    info!(
        "resolved: {} schemas, {} operations",
        doc.schemas.models.len(),
        doc.operations.len(),
    );

    let opts = specforge_core::docs::DocsOptions {
        out_dir: cli.out.clone(),
    };
    let written = specforge_core::docs::generate_docs(&doc, &opts)
        .context("failed to generate documentation")?;
    info!("generated {} files in {}", written.len(), cli.out.display());
    Ok(())
}

fn run_test(cli: &TestArgs) -> Result<usize> {
    info!("reading spec: {}", cli.spec.display());
    let spec = parse_file(&cli.spec)
        .with_context(|| format!("failed to parse spec at {}", cli.spec.display()))?;

    info!("resolving document into IR");
    let doc = resolve(&spec).context("failed to resolve spec into IR")?;

    info!(
        "resolved: {} schemas, {} operations",
        doc.schemas.models.len(),
        doc.operations.len(),
    );

    let lang = match cli.lang {
        Lang::Ts => specforge_core::TestLang::TypeScript,
        Lang::Go => specforge_core::TestLang::Go,
        Lang::Rust => specforge_core::TestLang::Rust,
    };

    info!(
        "generating {:?} test file(s) to: {}",
        lang,
        cli.out.display()
    );

    let opts = specforge_core::TestGenOptions { lang };
    let test_code = specforge_core::generate_tests(&doc, &opts);

    std::fs::create_dir_all(&cli.out)
        .with_context(|| format!("failed to create output directory {}", cli.out.display()))?;

    let filename = match lang {
        specforge_core::TestLang::TypeScript => "test.ts",
        specforge_core::TestLang::Go => "client_test.go",
        specforge_core::TestLang::Rust => "integration.rs",
    };

    let path = cli.out.join(filename);
    std::fs::write(&path, &test_code)
        .with_context(|| format!("failed to write {}", path.display()))?;

    eprintln!("Wrote {}", path.display());
    Ok(1)
}

fn run_versions(cli: VersionsArgs) -> Result<()> {
    let path = &cli.spec;

    if path.is_file() {
        // Single file: show its version from the info block.
        info!("reading spec: {}", path.display());
        let spec = parse_file(path)
            .with_context(|| format!("failed to parse spec at {}", path.display()))?;
        eprintln!("version: {}", spec.info.version);
        eprintln!("file:    {}", path.display());
        return Ok(());
    }

    if !path.is_dir() {
        bail!("path does not exist: {}", path.display());
    }

    info!("scanning directory: {}", path.display());
    let versions = scan_versions(path);

    if versions.is_empty() {
        bail!("no OpenAPI spec files found in {}", path.display());
    }

    // Print a table: version | file
    let max_ver_len = versions.iter().map(|v| v.version.len()).max().unwrap_or(7);
    let ver_header = "VERSION";
    let file_header = "FILE";
    eprintln!(
        "{:<width$}  {}",
        ver_header,
        file_header,
        width = max_ver_len.max(ver_header.len())
    );
    eprintln!(
        "{:-<width$}  {:-<40}",
        "",
        "",
        width = max_ver_len.max(ver_header.len())
    );
    for info in &versions {
        eprintln!(
            "{:<width$}  {}",
            info.version,
            info.path.display(),
            width = max_ver_len.max(ver_header.len())
        );
    }

    eprintln!("\n{} version(s) found", versions.len());
    Ok(())
}

fn run_convert(cli: &ConvertArgs) -> Result<()> {
    info!("reading spec: {}", cli.spec.display());
    let bytes = std::fs::read(&cli.spec)
        .with_context(|| format!("failed to read spec at {}", cli.spec.display()))?;
    let text = std::str::from_utf8(&bytes).context("spec is not valid UTF-8")?;

    // Parse as raw JSON/YAML value (not into openapiv3 types) to preserve the full structure.
    let mut json: JsonValue = if let Ok(v) = serde_json::from_str::<JsonValue>(text.trim_start()) {
        v
    } else {
        serde_yaml::from_str::<JsonValue>(text).context("failed to parse spec as JSON or YAML")?
    };

    // Detect current version.
    let current_version = json
        .get("openapi")
        .and_then(|v| v.as_str())
        .unwrap_or("3.0.3")
        .to_string();

    match cli.to {
        OpenApiVersion::V31 => {
            // 3.0 → 3.1 conversion
            if !current_version.starts_with("3.1") {
                info!("converting from {} to 3.1", current_version);
                upgrade_30_to_31(&mut json);
            } else {
                info!("spec is already 3.1.x, no conversion needed");
            }
            // Set the version field.
            if let Some(obj) = json.as_object_mut() {
                obj.insert(
                    "openapi".to_string(),
                    JsonValue::String("3.1.0".to_string()),
                );
            }
        }
        OpenApiVersion::V30 => {
            // 3.1 → 3.0 conversion
            if current_version.starts_with("3.1") {
                info!("converting from 3.1 to 3.0");
                downgrade_31_to_30(&mut json);
            } else {
                info!("spec is already 3.0.x, no conversion needed");
            }
            // Set the version field.
            if let Some(obj) = json.as_object_mut() {
                obj.insert(
                    "openapi".to_string(),
                    JsonValue::String("3.0.3".to_string()),
                );
            }
        }
    }

    // Determine output format: JSON if input was JSON, YAML otherwise.
    let is_json_input = serde_json::from_str::<JsonValue>(text.trim_start()).is_ok();
    let output = if is_json_input {
        serde_json::to_string_pretty(&json).context("failed to serialize")?
    } else {
        serde_yaml::to_string(&json).context("failed to serialize as YAML")?
    };

    match &cli.out {
        Some(path) => {
            std::fs::write(path, &output)
                .with_context(|| format!("failed to write {}", path.display()))?;
            eprintln!("Wrote {}", path.display());
        }
        None => {
            println!("{output}");
        }
    }

    Ok(())
}

fn run_merge(cli: &MergeArgs) -> Result<()> {
    let mut values = Vec::new();
    for path in &cli.specs {
        info!("reading spec: {}", path.display());
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read spec at {}", path.display()))?;
        let val: serde_json::Value = if path.extension().is_some_and(|e| e == "json") {
            serde_json::from_str(&text)
                .with_context(|| format!("failed to parse {} as JSON", path.display()))?
        } else {
            serde_yaml::from_str(&text)
                .with_context(|| format!("failed to parse {} as YAML", path.display()))?
        };
        values.push(val);
    }

    info!("merging {} spec(s)", values.len());
    let merged = merge_specs(&values)?;

    let output = match cli.format.as_str() {
        "json" => serde_json::to_string_pretty(&merged)
            .context("failed to serialize merged spec as JSON")?,
        _ => serde_yaml::to_string(&merged).context("failed to serialize merged spec as YAML")?,
    };

    match &cli.out {
        Some(path) => {
            std::fs::write(path, &output)
                .with_context(|| format!("failed to write {}", path.display()))?;
            eprintln!("Wrote {}", path.display());
        }
        None => {
            println!("{output}");
        }
    }

    Ok(())
}

fn run_workspace(cli: &WorkspaceArgs) -> Result<specforge_core::WorkspaceRunResult> {
    let config = specforge_core::WorkspaceConfig::load(&cli.config).with_context(|| {
        format!(
            "failed to load workspace config from {}",
            cli.config.display()
        )
    })?;

    info!(
        "loaded workspace: {} spec(s) from {}",
        config.specs.len(),
        cli.config.display()
    );

    let mut specs_processed = 0usize;
    let mut outputs_generated = 0usize;
    let mut files_written = 0usize;

    for spec_config in &config.specs {
        // Filter by --only if specified.
        if let Some(ref only) = cli.only {
            if &spec_config.name != only {
                continue;
            }
        }

        specs_processed += 1;
        info!("generating: {}", spec_config.name);

        let spec_path = spec_config.resolved_spec_path(&cli.config);

        if cli.dry_run {
            for output in &spec_config.outputs {
                let display_name = output.name.as_deref().unwrap_or("(default)");
                eprintln!("  {} -> {} ({})", output.lang, output.out, display_name);
                outputs_generated += 1;
            }
            continue;
        }

        // Parse and resolve the spec once per spec entry.
        let spec = parse_file(&spec_path).with_context(|| {
            format!(
                "[{}] failed to parse spec at {}",
                spec_config.name,
                spec_path.display()
            )
        })?;
        let doc = resolve(&spec)
            .with_context(|| format!("[{}] failed to resolve spec into IR", spec_config.name))?;

        for output in &spec_config.outputs {
            let out_path = output.resolved_out_path(&cli.config);
            let lang = output.lang.as_str();

            info!("  {} -> {}", lang, out_path.display());

            let written = match lang {
                "ts" | "typescript" => {
                    let opts = specforge_ts::GeneratorOptions {
                        out_dir: out_path.clone(),
                        package_name: output.name.clone(),
                        i18n: None,
                    };
                    specforge_ts::generate(&doc, &opts).with_context(|| {
                        format!("[{}] failed to emit TypeScript SDK", spec_config.name)
                    })?
                }
                "go" => {
                    let opts = specforge_go::GeneratorOptions {
                        out_dir: out_path.clone(),
                        module_path: output.name.clone(),
                        package_name: None,
                        i18n: None,
                    };
                    specforge_go::generate(&doc, &opts)
                        .with_context(|| format!("[{}] failed to emit Go SDK", spec_config.name))?
                }
                "rust" => {
                    let opts = specforge_rust::GeneratorOptions {
                        out_dir: out_path.clone(),
                        crate_name: output.name.clone(),
                        i18n: None,
                    };
                    specforge_rust::generate(&doc, &opts).with_context(|| {
                        format!("[{}] failed to emit Rust SDK", spec_config.name)
                    })?
                }
                other => {
                    bail!("[{}] unsupported language: {}", spec_config.name, other);
                }
            };

            outputs_generated += 1;
            files_written += written.len();
        }
    }

    if specs_processed == 0 {
        if cli.only.is_some() {
            bail!("no spec matched --only filter; check the name in your workspace config");
        }
        bail!("workspace config contains no specs");
    }

    Ok(specforge_core::WorkspaceRunResult {
        specs_processed,
        outputs_generated,
        files_written,
    })
}

fn run_workspace_init(cli: &WorkspaceInitArgs) -> Result<specforge_core::WorkspaceInitResult> {
    let result = specforge_core::init_workspace(&cli.dir, &cli.out)
        .context("failed to initialize workspace")?;

    if result.specs_found == 0 {
        eprintln!(
            "warning: no OpenAPI spec files found in {}",
            cli.dir.display()
        );
    }

    Ok(result)
}

fn run_migrate(cli: &MigrateArgs) -> Result<()> {
    info!("reading old spec: {}", cli.old.display());
    let old_spec = parse_file(&cli.old)
        .with_context(|| format!("failed to parse old spec at {}", cli.old.display()))?;
    let old_doc = resolve(&old_spec).context("failed to resolve old spec")?;

    info!("reading new spec: {}", cli.new.display());
    let new_spec = parse_file(&cli.new)
        .with_context(|| format!("failed to parse new spec at {}", cli.new.display()))?;
    let new_doc = resolve(&new_spec).context("failed to resolve new spec")?;

    let old_version = if old_spec.info.version.is_empty() {
        "old".to_string()
    } else {
        old_spec.info.version.clone()
    };
    let new_version = if new_spec.info.version.is_empty() {
        "new".to_string()
    } else {
        new_spec.info.version.clone()
    };

    let guide =
        specforge_core::generate_migration_guide(&old_doc, &new_doc, &old_version, &new_version);

    match &cli.out {
        Some(path) => {
            std::fs::write(path, &guide)
                .with_context(|| format!("failed to write {}", path.display()))?;
            eprintln!("Wrote {}", path.display());
        }
        None => {
            println!("{guide}");
        }
    }

    Ok(())
}

fn run_analyze(cli: &AnalyzeArgs) -> Result<()> {
    info!("reading spec: {}", cli.spec.display());
    let spec = parse_file(&cli.spec)
        .with_context(|| format!("failed to parse spec at {}", cli.spec.display()))?;
    info!("resolving document into IR");
    let doc = resolve(&spec).context("failed to resolve spec into IR")?;
    info!(
        "resolved: {} schemas, {} operations",
        doc.schemas.models.len(),
        doc.operations.len()
    );
    let report = specforge_core::analyze_spec(&doc);
    match cli.format {
        AnalyzeFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&report).context("failed to serialize")?
            );
        }
        AnalyzeFormat::Markdown => {
            print_analysis_markdown(&report);
        }
        AnalyzeFormat::Text => {
            print_analysis_text(&report);
        }
    }
    Ok(())
}

fn run_infer(cli: &InferArgs) -> Result<()> {
    // Read JSON from file or stdin.
    let json_text = if cli.input == "-" {
        let mut buf = String::new();
        std::io::Read::read_to_string(&mut std::io::stdin(), &mut buf)
            .context("failed to read JSON from stdin")?;
        buf
    } else {
        let path = PathBuf::from(&cli.input);
        info!("reading sample JSON: {}", path.display());
        std::fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?
    };

    let json: serde_json::Value =
        serde_json::from_str(json_text.trim()).context("failed to parse input as JSON")?;

    info!("inferred schema for {}-key object", {
        match &json {
            serde_json::Value::Object(m) => m.len(),
            serde_json::Value::Array(a) => a.len(),
            _ => 0,
        }
    });

    let opts = specforge_core::InferOptions {
        schema_name: cli.name.clone(),
        title: cli.title.clone(),
        version: cli.version.clone(),
    };

    let spec = specforge_core::infer_openapi(&json, &opts);

    let output =
        serde_json::to_string_pretty(&spec).context("failed to serialize inferred spec")?;

    match &cli.out {
        Some(path) => {
            std::fs::write(path, &output)
                .with_context(|| format!("failed to write {}", path.display()))?;
            eprintln!("Wrote {}", path.display());
        }
        None => {
            println!("{output}");
        }
    }

    Ok(())
}

fn run_verify(cli: &VerifyArgs) -> Result<Vec<specforge_core::VerifyResult>> {
    info!("reading spec: {}", cli.spec.display());
    let spec = parse_file(&cli.spec)
        .with_context(|| format!("failed to parse spec at {}", cli.spec.display()))?;

    info!("resolving document into IR");
    let doc = resolve(&spec).context("failed to resolve spec into IR")?;

    info!(
        "resolved: {} schemas, {} operations",
        doc.schemas.models.len(),
        doc.operations.len(),
    );

    let opts = specforge_core::VerifyOptions {
        base_url: cli.base_url.clone(),
        auth: cli.auth.clone(),
        timeout_ms: cli.timeout,
    };

    info!("verifying against {}", cli.base_url);
    let results = specforge_core::verify_api(&doc, &opts);

    // Print results.
    let mut total = 0usize;
    let mut passed = 0usize;
    let mut failed = 0usize;

    for result in &results {
        total += 1;
        let has_issues = !result.issues.is_empty();
        if has_issues {
            failed += 1;
            eprintln!(
                "FAIL  {} {} ({})",
                result.method, result.endpoint, result.status
            );
            for issue in &result.issues {
                eprintln!("      - {issue}");
            }
        } else {
            passed += 1;
            eprintln!(
                "  OK  {} {} ({})",
                result.method, result.endpoint, result.status
            );
        }
    }

    eprintln!("\n{passed}/{total} endpoint(s) passed, {failed} failed");

    let json = serde_json::to_string_pretty(&results).context("failed to serialize results")?;
    println!("{json}");

    Ok(results)
}

fn print_analysis_text(report: &specforge_core::AnalysisReport) {
    eprintln!("=== Spec Bundle Analysis ===");
    eprintln!();
    eprintln!("Schemas:     {}", report.total_schemas);
    eprintln!("Operations:  {}", report.total_operations);
    eprintln!(
        "IR size:     {:.1} KB",
        report.total_size_bytes as f64 / 1024.0
    );
    eprintln!();
    if !report.unused_schemas.is_empty() {
        eprintln!("Unused schemas ({}):", report.unused_schemas.len());
        for name in &report.unused_schemas {
            eprintln!("  - {name}");
        }
        eprintln!();
    }
    if !report.duplicate_schemas.is_empty() {
        eprintln!(
            "Duplicate schemas ({} pair(s)):",
            report.duplicate_schemas.len()
        );
        for (a, b) in &report.duplicate_schemas {
            eprintln!("  - {a} <-> {b}");
        }
        eprintln!();
    }
    if !report.large_schemas.is_empty() {
        eprintln!("Large schemas (>20 properties):");
        for (name, count) in &report.large_schemas {
            eprintln!("  - {name}: {count} properties");
        }
        eprintln!();
    }
    if !report.deep_refs.is_empty() {
        eprintln!("Deep reference chains:");
        for (name, depth) in &report.deep_refs {
            eprintln!("  - {name}: depth {depth}");
        }
        eprintln!();
    }
    if report.recommendations.is_empty() {
        eprintln!("No issues found. Spec looks healthy.");
    } else {
        eprintln!("Recommendations ({}):", report.recommendations.len());
        for (i, rec) in report.recommendations.iter().enumerate() {
            eprintln!("  {}. {rec}", i + 1);
        }
    }
}
fn print_analysis_markdown(report: &specforge_core::AnalysisReport) {
    println!("# Spec Bundle Analysis");
    println!();
    println!("| Metric | Value |");
    println!("|--------|-------|");
    println!("| Schemas | {} |", report.total_schemas);
    println!("| Operations | {} |", report.total_operations);
    println!(
        "| IR size | {:.1} KB |",
        report.total_size_bytes as f64 / 1024.0
    );
    println!();
    if !report.unused_schemas.is_empty() {
        println!("## Unused Schemas ({})", report.unused_schemas.len());
        println!();
        for name in &report.unused_schemas {
            println!("- `{name}`");
        }
        println!();
    }
    if !report.duplicate_schemas.is_empty() {
        println!(
            "## Duplicate Schemas ({} pair(s))",
            report.duplicate_schemas.len()
        );
        println!();
        for (a, b) in &report.duplicate_schemas {
            println!("- `{a}` <-> `{b}`");
        }
        println!();
    }
    if !report.large_schemas.is_empty() {
        println!("## Large Schemas (>20 properties)");
        println!();
        for (name, count) in &report.large_schemas {
            println!("- `{name}`: {count} properties");
        }
        println!();
    }
    if !report.deep_refs.is_empty() {
        println!("## Deep Reference Chains");
        println!();
        for (name, depth) in &report.deep_refs {
            println!("- `{name}`: depth {depth}");
        }
        println!();
    }
    if !report.recommendations.is_empty() {
        println!("## Recommendations");
        println!();
        for (i, rec) in report.recommendations.iter().enumerate() {
            println!("{}. {rec}", i + 1);
        }
    } else {
        println!("No issues found. Spec looks healthy.");
    }
}
/// Recursively upgrade OpenAPI 3.0 constructs to 3.1 equivalents.
fn upgrade_30_to_31(json: &mut JsonValue) {
    match json {
        JsonValue::Object(obj) => {
            // Convert `type: X` + `nullable: true` → `type: ["X", "null"]`
            if let (Some(type_val), Some(nullable)) =
                (obj.get("type").cloned(), obj.get("nullable").cloned())
            {
                if nullable.as_bool() == Some(true) {
                    if let Some(type_str) = type_val.as_str() {
                        let arr = JsonValue::Array(vec![
                            JsonValue::String(type_str.to_string()),
                            JsonValue::String("null".to_string()),
                        ]);
                        obj.insert("type".to_string(), arr);
                    }
                    obj.remove("nullable");
                }
            }

            // Convert boolean `exclusiveMinimum`/`exclusiveMaximum` to numeric.
            // In 3.0 these are booleans (with minimum/maximum); in 3.1 they are numbers.
            for field in &["exclusiveMinimum", "exclusiveMaximum"] {
                if let Some(val) = obj.get(*field).cloned() {
                    if val.as_bool() == Some(true) {
                        // Find the corresponding minimum/maximum value.
                        let companion = if *field == "exclusiveMinimum" {
                            "minimum"
                        } else {
                            "maximum"
                        };
                        if let Some(limit) = obj.get(companion).cloned() {
                            if limit.is_number() {
                                obj.insert(field.to_string(), limit);
                            }
                        }
                    }
                }
            }

            // Convert single-element `enum: [value]` → `const: value`.
            // In 3.0, a single-value constraint is expressed as `enum: [X]`;
            // in 3.1, `const` is the idiomatic equivalent.
            if let Some(enum_val) = obj.get("enum").cloned() {
                if let Some(arr) = enum_val.as_array() {
                    if arr.len() == 1 {
                        obj.insert("const".to_string(), arr[0].clone());
                        obj.remove("enum");
                    }
                }
            }

            // Recurse into all values.
            for val in obj.values_mut() {
                upgrade_30_to_31(val);
            }
        }
        JsonValue::Array(arr) => {
            for val in arr.iter_mut() {
                upgrade_30_to_31(val);
            }
        }
        _ => {}
    }
}

/// Recursively downgrade OpenAPI 3.1 constructs to 3.0 equivalents.
fn downgrade_31_to_30(json: &mut JsonValue) {
    // Ensure `paths` exists (optional in 3.1, required in 3.0).
    if json.get("paths").is_none() {
        if let Some(obj) = json.as_object_mut() {
            obj.insert("paths".to_string(), JsonValue::Object(Default::default()));
        }
    }
    downgrade_value(json);
}

/// Recursively walk and transform a JSON value (3.1 → 3.0).
fn downgrade_value(json: &mut JsonValue) {
    match json {
        JsonValue::Object(obj) => {
            // Convert `type: ["X", "null"]` → `type: X` + `nullable: true`.
            if let Some(type_val) = obj.get("type").cloned() {
                if let Some(arr) = type_val.as_array() {
                    let has_null = arr.iter().any(|v| v.as_str() == Some("null"));
                    let non_null: Vec<&JsonValue> =
                        arr.iter().filter(|v| v.as_str() != Some("null")).collect();
                    if has_null && non_null.len() == 1 {
                        obj.insert("type".to_string(), non_null[0].clone());
                        obj.insert("nullable".to_string(), JsonValue::Bool(true));
                    } else if has_null && non_null.is_empty() {
                        obj.remove("type");
                        obj.insert("nullable".to_string(), JsonValue::Bool(true));
                    }
                }
            }

            // Convert numeric `exclusive_minimum` / `exclusive_maximum` → boolean.
            for field in &["exclusiveMinimum", "exclusiveMaximum"] {
                if let Some(val) = obj.get(*field).cloned() {
                    if val.is_number() {
                        obj.insert(field.to_string(), JsonValue::Bool(true));
                    }
                }
            }

            // Convert `const` → `enum: [value]`.
            if let Some(const_val) = obj.remove("const") {
                obj.insert("enum".to_string(), JsonValue::Array(vec![const_val]));
            }

            // Convert `dependentRequired` → merge into `required`.
            if let Some(dep_req) = obj.remove("dependentRequired") {
                if let Some(dep_map) = dep_req.as_object() {
                    let mut extra: Vec<String> = Vec::new();
                    for deps in dep_map.values() {
                        if let Some(arr) = deps.as_array() {
                            for v in arr {
                                if let Some(s) = v.as_str() {
                                    extra.push(s.to_string());
                                }
                            }
                        }
                    }
                    if !extra.is_empty() {
                        let required = obj
                            .entry("required".to_string())
                            .or_insert_with(|| JsonValue::Array(vec![]));
                        if let Some(req_arr) = required.as_array_mut() {
                            for dep in extra {
                                let dep_json = JsonValue::String(dep);
                                if !req_arr.contains(&dep_json) {
                                    req_arr.push(dep_json);
                                }
                            }
                        }
                    }
                }
            }

            // Convert `prefixItems` → `items` (tuple → array).
            if let Some(prefix) = obj.remove("prefixItems") {
                if let Some(arr) = prefix.as_array() {
                    if !arr.is_empty() {
                        obj.insert("items".to_string(), arr[0].clone());
                    }
                }
            }

            // Recurse into all values.
            for val in obj.values_mut() {
                downgrade_value(val);
            }
        }
        JsonValue::Array(arr) => {
            for val in arr.iter_mut() {
                downgrade_value(val);
            }
        }
        _ => {}
    }
}

fn run_evolution(cli: &EvolutionArgs) -> Result<()> {
    let spec_str = cli.spec.to_str().unwrap_or("spec");

    info!("tracking evolution for: {}", spec_str);
    let mut evo = specforge_core::track_evolution(spec_str)
        .map_err(|e| anyhow::anyhow!("{e}"))
        .context("failed to track schema evolution")?;

    // Apply limit: keep the most recent N versions.
    if evo.versions.len() > cli.limit {
        evo.versions = evo.versions.split_off(evo.versions.len() - cli.limit);
    }

    match cli.format {
        EvolutionFormat::Text => {
            print!("{}", specforge_core::evolution_format_text(&evo));
        }
        EvolutionFormat::Json => {
            let json = specforge_core::evolution_format_json(&evo)
                .context("failed to serialize evolution as JSON")?;
            println!("{json}");
        }
        EvolutionFormat::Markdown => {
            print!("{}", specforge_core::evolution_format_markdown(&evo));
        }
    }

    Ok(())
}

fn run_mock(cli: &MockArgs) -> Result<()> {
    info!("reading spec: {}", cli.spec.display());
    let spec = parse_file(&cli.spec)
        .with_context(|| format!("failed to parse spec at {}", cli.spec.display()))?;

    info!("resolving document into IR");
    let doc = resolve(&spec).context("failed to resolve spec into IR")?;

    info!(
        "resolved: {} schemas, {} operations",
        doc.schemas.models.len(),
        doc.operations.len(),
    );

    let mut server = specforge_core::MockServer::from_doc(&doc).host(&cli.host);

    if let Some(port) = cli.port {
        server = server.port(port);
    }

    let port = server.start().context("failed to bind mock server")?;

    eprintln!("Mock server listening on http://{}:{}", cli.host, port);
    eprintln!("Routes:");
    for op in &doc.operations {
        eprintln!("  {} {} -> {}", op.method.upper(), op.path, op.operation_id,);
    }
    eprintln!("\nPress Ctrl+C to stop.");

    server.serve();
    Ok(())
}

fn run_export(cli: &ExportArgs) -> Result<()> {
    info!("reading spec: {}", cli.spec.display());
    let spec_text = std::fs::read_to_string(&cli.spec)
        .with_context(|| format!("failed to read spec at {}", cli.spec.display()))?;

    match cli.format {
        ExportFormat::SwaggerEditor => {
            let options = specforge_core::ExportOptions::default();
            let output = specforge_core::export_spec(&spec_text, &options)
                .context("failed to export spec for Swagger Editor")?;

            match &cli.out {
                Some(path) => {
                    std::fs::write(path, &output)
                        .with_context(|| format!("failed to write {}", path.display()))?;
                    eprintln!("Wrote {}", path.display());
                }
                None => {
                    println!("{output}");
                }
            }
        }
    }

    Ok(())
}

fn run_demo(cli: &DemoArgs) -> Result<()> {
    let output = specforge_core::generate_demo_spec();

    match &cli.out {
        Some(path) => {
            std::fs::write(path, &output)
                .with_context(|| format!("failed to write {}", path.display()))?;
            eprintln!("Wrote {}", path.display());
        }
        None => {
            println!("{output}");
        }
    }

    Ok(())
}

fn run_changelog(cli: &ChangelogArgs) -> Result<()> {
    info!("reading spec: {}", cli.spec.display());
    let spec = parse_file(&cli.spec)
        .with_context(|| format!("failed to parse spec at {}", cli.spec.display()))?;

    info!("resolving document into IR");
    let doc = resolve(&spec).context("failed to resolve spec into IR")?;

    let fmt = match cli.format {
        ChangelogOutputFormat::Markdown => ChangelogFormat::Markdown,
        ChangelogOutputFormat::Json => ChangelogFormat::Json,
    };

    let opts = ChangelogOptions {
        version: cli.version.clone(),
        previous_spec: cli.previous.as_ref().map(|p| p.display().to_string()),
        suggest_version: cli.suggest_version,
        format: fmt,
    };

    let output = generate_changelog(&doc, &opts);

    // Determine the default filename based on output format.
    let default_filename = match cli.format {
        ChangelogOutputFormat::Markdown => "CHANGELOG.md",
        ChangelogOutputFormat::Json => "CHANGELOG.json",
    };

    match &cli.out {
        Some(path) => {
            std::fs::write(path, &output)
                .with_context(|| format!("failed to write {}", path.display()))?;
            eprintln!("Wrote {}", path.display());
        }
        None => {
            let default_path = std::path::PathBuf::from(default_filename);
            std::fs::write(&default_path, &output)
                .with_context(|| format!("failed to write {}", default_path.display()))?;
            eprintln!("Wrote {}", default_path.display());
        }
    }

    Ok(())
}

fn run_market(cli: &MarketArgs) -> Result<()> {
    let mut index = MarketplaceIndex::built_in();

    // Merge an extra index if provided.
    if let Some(ref extra_path) = cli.extra_index {
        let extra = MarketplaceIndex::load(extra_path)
            .with_context(|| format!("failed to load extra index from {}", extra_path.display()))?;
        index.merge(&extra);
    }

    match &cli.market_cmd {
        MarketCommands::Search(args) => run_market_search(&index, args),
        MarketCommands::List(args) => run_market_list(&index, args),
        MarketCommands::Info(args) => run_market_info(&index, args),
        MarketCommands::Add(args) => run_market_add(&index, args),
    }
}

fn run_market_search(index: &MarketplaceIndex, cli: &MarketSearchArgs) -> Result<()> {
    let results = index.search(&cli.query);

    if results.is_empty() {
        eprintln!("No specs found matching \"{}\".", cli.query);
        return Ok(());
    }

    match cli.format.as_str() {
        "json" => {
            println!(
                "{}",
                serde_json::to_string_pretty(&results).context("failed to serialize")?
            );
        }
        _ => {
            eprintln!(
                "Found {} spec(s) matching \"{}\":\n",
                results.len(),
                cli.query
            );
            for entry in &results {
                let verified = if entry.verified { " [verified]" } else { "" };
                eprintln!(
                    "  {} v{}{}\n    {} (by {})\n    Downloads: {} | Rating: {:.1}/5\n    Tags: {}\n",
                    entry.name,
                    entry.version,
                    verified,
                    truncate(&entry.description, 80),
                    entry.author,
                    entry.downloads,
                    entry.rating,
                    entry.tags.join(", "),
                );
            }
        }
    }

    Ok(())
}

fn run_market_list(index: &MarketplaceIndex, cli: &MarketListArgs) -> Result<()> {
    let entries: Vec<&specforge_core::SpecEntry> = if let Some(ref tag) = cli.tag {
        let q = tag.to_lowercase();
        index
            .entries
            .iter()
            .filter(|e| e.tags.iter().any(|t| t.to_lowercase() == q))
            .collect()
    } else {
        index.sorted_by_downloads()
    };

    if entries.is_empty() {
        eprintln!("No specs found.");
        return Ok(());
    }

    match cli.format.as_str() {
        "json" => {
            println!(
                "{}",
                serde_json::to_string_pretty(&entries).context("failed to serialize")?
            );
        }
        _ => {
            eprintln!("Spec Marketplace -- {} spec(s)\n", entries.len());
            let name_w = entries
                .iter()
                .map(|e| e.name.len())
                .max()
                .unwrap_or(20)
                .max(20);
            eprintln!(
                "  {:<width$}  {:>10}  {:>5}  DESCRIPTION",
                "NAME",
                "DOWNLOADS",
                "RATING",
                width = name_w,
            );
            eprintln!(
                "  {:─<width$}  {:─>10}  {:─>5}  {width:─<40}",
                "",
                "",
                "",
                width = name_w
            );
            for entry in &entries {
                let verified = if entry.verified { " [v]" } else { "" };
                eprintln!(
                    "  {:<width$}  {:>10}  {:>5.1}  {}{}",
                    entry.name,
                    entry.downloads,
                    entry.rating,
                    truncate(&entry.description, 50),
                    verified,
                    width = name_w,
                );
            }
        }
    }

    Ok(())
}

fn run_market_info(index: &MarketplaceIndex, cli: &MarketInfoArgs) -> Result<()> {
    let entry = index
        .find(&cli.name)
        .ok_or_else(|| anyhow::anyhow!("spec not found: {}", cli.name))?;

    eprintln!("{}\n", entry.name);
    eprintln!("  Version:      {}", entry.version);
    eprintln!("  Author:       {}", entry.author);
    eprintln!("  Description:  {}", entry.description);
    eprintln!("  Downloads:    {}", entry.downloads);
    eprintln!("  Rating:       {:.1}/5", entry.rating);
    eprintln!(
        "  Verified:     {}",
        if entry.verified { "Yes" } else { "No" }
    );
    eprintln!("  Tags:         {}", entry.tags.join(", "));
    if !entry.url.is_empty() {
        eprintln!("  URL:          {}", entry.url);
    }
    eprintln!("  Spec URL:     {}", entry.spec_url);

    Ok(())
}

fn run_market_add(index: &MarketplaceIndex, cli: &MarketAddArgs) -> Result<()> {
    let spec_path = &cli.path;
    if !spec_path.exists() {
        bail!("file not found: {}", spec_path.display());
    }

    let entry = MarketplaceIndex::generate_metadata(spec_path)
        .with_context(|| format!("failed to generate metadata from {}", spec_path.display()))?;

    eprintln!("Generated metadata for: {}", entry.name);
    eprintln!("  Version:     {}", entry.version);
    eprintln!("  Description: {}", entry.description);

    // Build a new index: start from existing built-in entries and append.
    let mut new_index = index.clone();
    new_index.entries.push(entry);
    new_index
        .entries
        .sort_by_key(|a| std::cmp::Reverse(a.downloads));

    let json = serde_json::to_string_pretty(&new_index)
        .context("failed to serialize marketplace index")?;

    let out_path = &cli.out;
    std::fs::write(out_path, &json)
        .with_context(|| format!("failed to write {}", out_path.display()))?;
    eprintln!("Wrote {}", out_path.display());

    Ok(())
}

fn run_profile(cli: &ProfileArgs) -> Result<()> {
    info!("reading spec: {}", cli.spec.display());
    let spec = parse_file(&cli.spec)
        .with_context(|| format!("failed to parse spec at {}", cli.spec.display()))?;

    info!("resolving document into IR");
    let doc = resolve(&spec).context("failed to resolve spec into IR")?;

    info!(
        "resolved: {} schemas, {} operations",
        doc.schemas.models.len(),
        doc.operations.len(),
    );

    let opts = ProfileOptions {
        base_url: cli.base_url.clone(),
        auth: cli.auth.clone(),
        requests: cli.requests,
        concurrency: cli.concurrency,
        timeout_ms: cli.timeout,
        endpoint_filter: cli.endpoint.clone(),
    };

    info!(
        "profiling {} endpoint(s) against {} ({} requests each)",
        doc.operations.len(),
        cli.base_url,
        cli.requests,
    );

    let report = profile_api(&doc, &opts);

    match cli.format {
        ProfileFormat::Text => {
            print!("{}", specforge_core::profiler::format_text(&report));
        }
        ProfileFormat::Json => {
            let json = specforge_core::profiler::format_json(&report)
                .context("failed to serialize profile report as JSON")?;
            println!("{json}");
        }
        ProfileFormat::Markdown => {
            print!("{}", specforge_core::profiler::format_markdown(&report));
        }
    }

    Ok(())
}

fn run_version(cli: &VersionArgs) -> Result<()> {
    info!("reading spec: {}", cli.spec.display());
    let spec = parse_file(&cli.spec)
        .with_context(|| format!("failed to parse spec at {}", cli.spec.display()))?;

    info!("resolving document into IR");
    let mut doc = resolve(&spec).context("failed to resolve spec into IR")?;

    info!(
        "resolved: {} schemas, {} operations",
        doc.schemas.models.len(),
        doc.operations.len(),
    );

    let strategy = match cli.strategy {
        VersionStrategyArg::Url => VersionStrategy::UrlPath,
        VersionStrategyArg::Header => VersionStrategy::Header,
        VersionStrategyArg::Query => VersionStrategy::QueryParam,
        VersionStrategyArg::None => VersionStrategy::None,
    };

    let prefix = if matches!(
        strategy,
        VersionStrategy::UrlPath | VersionStrategy::QueryParam
    ) {
        Some(cli.prefix.clone())
    } else {
        None
    };

    let config = VersioningConfig {
        strategy,
        prefix,
        header_name: cli.header.clone(),
    };

    info!("applying versioning (strategy: {:?})", cli.strategy);
    apply_versioning(&mut doc, &config);

    for op in &doc.operations {
        info!("  {} {}", op.method.upper(), op.path);
    }

    let output =
        serde_json::to_string_pretty(&doc).context("failed to serialize versioned IR to JSON")?;

    match &cli.out {
        Some(path) => {
            std::fs::write(path, &output)
                .with_context(|| format!("failed to write {}", path.display()))?;
            eprintln!("Wrote {}", path.display());
        }
        None => {
            println!("{output}");
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Plugin path resolution
// ---------------------------------------------------------------------------

/// Resolve a plugin name to a filesystem path by checking:
/// 1. Local `.specforge.yaml` config for a matching plugin entry
/// 2. The built-in plugin marketplace index for a URL
/// 3. Common paths like `./plugins/<name>.wasm`
fn resolve_plugin_path(name: &str) -> Result<PathBuf> {
    // 1. Check .specforge.yaml
    let config_path = PathBuf::from(".specforge.yaml");
    if config_path.exists() {
        if let Ok(config) = SpecforgeConfig::load(&config_path) {
            if let Some(plugin) = config.find_plugin(name) {
                let path = PathBuf::from(&plugin.path);
                if path.exists() {
                    return Ok(path);
                }
                // Path configured but file missing — try relative to config dir.
                let base = config_path.parent().unwrap_or_else(|| Path::new("."));
                let resolved = base.join(&plugin.path);
                if resolved.exists() {
                    return Ok(resolved);
                }
                bail!(
                    "plugin '{name}' is configured in {} but {} not found",
                    config_path.display(),
                    plugin.path,
                );
            }
        }
    }

    // 2. Check the built-in marketplace index for a download URL.
    let index = PluginIndex::built_in();
    if let Some(_plugin) = index.find(name) {
        // Return the expected local install path.
        let local = PathBuf::from("plugins").join(format!("{name}.wasm"));
        if local.exists() {
            return Ok(local);
        }
        bail!(
            "plugin '{name}' is available in the marketplace but not installed.\n\
             Run: specforge plugin install {name}"
        );
    }

    // 3. Fallback: check common paths.
    let candidates = [
        PathBuf::from(format!("plugins/{name}.wasm")),
        PathBuf::from(format!("./{name}.wasm")),
    ];
    for candidate in &candidates {
        if candidate.exists() {
            return Ok(candidate.clone());
        }
    }

    bail!(
        "unknown plugin '{name}'. Available plugins can be listed with:\n\
         specforge plugin list"
    );
}

// ---------------------------------------------------------------------------
// Plugin subcommand handlers
// ---------------------------------------------------------------------------

fn load_plugin_index(extra_index: &Option<PathBuf>) -> Result<PluginIndex> {
    let mut index = PluginIndex::built_in();
    if let Some(ref extra_path) = extra_index {
        let extra = PluginIndex::load(extra_path).with_context(|| {
            format!(
                "failed to load extra plugin index from {}",
                extra_path.display()
            )
        })?;
        index.merge(&extra);
    }
    Ok(index)
}

fn run_plugin(cli: &PluginArgs) -> Result<()> {
    let index = load_plugin_index(&cli.extra_index)?;

    match &cli.plugin_cmd {
        PluginCommands::List(args) => run_plugin_list(&index, args),
        PluginCommands::Search(args) => run_plugin_search(&index, args),
        PluginCommands::Info(args) => run_plugin_info(&index, args),
        PluginCommands::Install(args) => run_plugin_install(&index, args),
    }
}

fn run_plugin_list(index: &PluginIndex, cli: &PluginListArgs) -> Result<()> {
    let plugins: Vec<&specforge_core::PluginEntry> = if let Some(ref lang) = cli.language {
        let q = lang.to_lowercase();
        index
            .plugins
            .iter()
            .filter(|p| p.language.to_lowercase() == q)
            .collect()
    } else {
        index.sorted_by_downloads()
    };

    if plugins.is_empty() {
        eprintln!("No plugins found.");
        return Ok(());
    }

    match cli.format.as_str() {
        "json" => {
            println!(
                "{}",
                serde_json::to_string_pretty(&plugins).context("failed to serialize")?
            );
        }
        _ => {
            eprintln!("Plugin Marketplace -- {} plugin(s)\n", plugins.len());
            let name_w = plugins
                .iter()
                .map(|p| p.name.len())
                .max()
                .unwrap_or(20)
                .max(20);
            eprintln!(
                "  {:<width$}  {:>10}  {:>5}  {:>12}  DESCRIPTION",
                "NAME",
                "DOWNLOADS",
                "RATING",
                "LANGUAGE",
                width = name_w,
            );
            eprintln!(
                "  {:─<width$}  {:─>10}  {:─>5}  {:─>12}  {width:─<40}",
                "",
                "",
                "",
                "",
                width = name_w
            );
            for p in &plugins {
                let verified = if p.verified { " [v]" } else { "" };
                eprintln!(
                    "  {:<width$}  {:>10}  {:>5.1}  {:>12}  {}{}",
                    p.name,
                    p.downloads,
                    p.rating,
                    p.language,
                    truncate(&p.description, 45),
                    verified,
                    width = name_w,
                );
            }
        }
    }

    Ok(())
}

fn run_plugin_search(index: &PluginIndex, cli: &PluginSearchArgs) -> Result<()> {
    let results = index.search(&cli.query);

    if results.is_empty() {
        eprintln!("No plugins found matching \"{}\".", cli.query);
        return Ok(());
    }

    match cli.format.as_str() {
        "json" => {
            println!(
                "{}",
                serde_json::to_string_pretty(&results).context("failed to serialize")?
            );
        }
        _ => {
            eprintln!(
                "Found {} plugin(s) matching \"{}\":\n",
                results.len(),
                cli.query
            );
            for p in &results {
                let verified = if p.verified { " [verified]" } else { "" };
                eprintln!(
                    "  {} v{}{}\n    {} (by {})\n    Language: {} | Downloads: {} | Rating: {:.1}/5\n",
                    p.name,
                    p.version,
                    verified,
                    truncate(&p.description, 80),
                    p.author,
                    p.language,
                    p.downloads,
                    p.rating,
                );
            }
        }
    }

    Ok(())
}

fn run_plugin_info(index: &PluginIndex, cli: &PluginInfoArgs) -> Result<()> {
    let plugin = index
        .find(&cli.name)
        .ok_or_else(|| anyhow::anyhow!("plugin not found: {}", cli.name))?;

    eprintln!("{}\n", plugin.name);
    eprintln!("  Version:      {}", plugin.version);
    eprintln!("  Author:       {}", plugin.author);
    eprintln!("  Description:  {}", plugin.description);
    eprintln!("  Language:     {}", plugin.language);
    eprintln!("  Downloads:    {}", plugin.downloads);
    eprintln!("  Rating:       {:.1}/5", plugin.rating);
    eprintln!(
        "  Verified:     {}",
        if plugin.verified { "Yes" } else { "No" }
    );
    if !plugin.url.is_empty() {
        eprintln!("  URL:          {}", plugin.url);
    }

    Ok(())
}

fn run_plugin_install(index: &PluginIndex, cli: &PluginInstallArgs) -> Result<()> {
    let dest = index
        .install_plugin(&cli.name, &cli.dir)
        .with_context(|| format!("failed to install plugin {}", cli.name))?;

    eprintln!("Installed plugin '{}' to {}", cli.name, dest.display());

    // Also show the .specforge.yaml snippet the user can add.
    eprintln!("\nAdd to your .specforge.yaml:");
    eprintln!(
        "  plugins:\n    - name: {}\n      path: {}\n      enabled: true",
        cli.name,
        dest.display(),
    );

    Ok(())
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len - 3])
    }
}

//! `specforge` -- CLI entry point.
//!
//! Reads an OpenAPI YAML/JSON spec, runs the full pipeline (parse -> resolve ->
//! IR -> emit), and writes a ready-to-build SDK for the chosen target language.

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{bail, Context, Result};
use clap::{Args, Parser, Subcommand, ValueEnum};
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

use specforge_core::{diff, lint, parse_file, resolve, DiffSeverity, Severity};

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

    /// Log verbosity.
    #[arg(short = 'v', long = "log-level", value_enum, default_value_t = LogLevel::Info)]
    log_level: LogLevel,
}

#[derive(Args, Debug)]
struct CheckArgs {
    /// Path to the OpenAPI spec (YAML or JSON).
    spec: PathBuf,

    /// Treat warnings as errors.
    #[arg(long)]
    strict: bool,

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

fn main() -> ExitCode {
    let cli = Cli::parse();

    let level = match &cli.command {
        Commands::Generate(args) => args.log_level,
        Commands::Check(args) => args.log_level,
        Commands::Diff(args) => args.log_level,
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
                let _ = warn!("{e:#}");
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
                let _ = warn!("{e:#}");
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
                let _ = warn!("{e:#}");
                ExitCode::FAILURE
            }
        },
    }
}

fn run_generate(cli: GenerateArgs) -> Result<usize> {
    info!("reading spec: {}", cli.spec.display());
    let spec = parse_file(&cli.spec)
        .with_context(|| format!("failed to parse spec at {}", cli.spec.display()))?;

    info!("resolving document into IR");
    let doc = resolve(&spec).context("failed to resolve spec into IR")?;

    info!(
        "resolved: {} schemas, {} operations, {} security scheme(s)",
        doc.schemas.models.len(),
        doc.operations.len(),
        doc.security.len(),
    );

    info!(
        "emitting {:?} SDK to: {}",
        cli.lang,
        cli.out.display()
    );

    let written = match cli.lang {
        Lang::Ts => {
            let opts = specforge_ts::GeneratorOptions {
                out_dir: cli.out.clone(),
                package_name: cli.package_name.clone(),
            };
            specforge_ts::generate(&doc, &opts).context("failed to emit TypeScript SDK")?
        }
        Lang::Go => {
            let opts = specforge_go::GeneratorOptions {
                out_dir: cli.out.clone(),
                module_path: cli.package_name.clone(),
                package_name: None,
            };
            specforge_go::generate(&doc, &opts).context("failed to emit Go SDK")?
        }
        Lang::Rust => {
            let opts = specforge_rust::GeneratorOptions {
                out_dir: cli.out.clone(),
                crate_name: cli.package_name.clone(),
            };
            specforge_rust::generate(&doc, &opts).context("failed to emit Rust SDK")?
        }
    };

    if written.is_empty() {
        bail!("emitter wrote zero files");
    }
    Ok(written.len())
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

    let diagnostics = lint::lint(&doc);

    let mut has_errors = false;
    for diag in &diagnostics {
        let is_error = diag.severity == Severity::Error || (cli.strict && diag.severity == Severity::Warning);
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
        let errors = diagnostics.iter().filter(|d| d.severity == Severity::Error).count();
        let warnings = diagnostics.len() - errors;
        if errors > 0 {
            eprintln!(
                "\n{errors} error(s), {warnings} warning(s)",
            );
        } else if cli.strict && warnings > 0 {
            eprintln!(
                "\n{warnings} warning(s) treated as errors (--strict)",
            );
            has_errors = true;
        } else {
            eprintln!(
                "\n{warnings} warning(s)",
            );
        }
    }

    Ok(has_errors)
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

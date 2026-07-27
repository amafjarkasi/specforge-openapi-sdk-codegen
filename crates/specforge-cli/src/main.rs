//! `specforge` -- CLI entry point.
//!
//! Reads an OpenAPI YAML/JSON spec, runs the full pipeline (parse -> resolve ->
//! IR -> emit), and writes a ready-to-build SDK for the chosen target language.

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{bail, Context, Result};
use clap::{Args, Parser, Subcommand, ValueEnum};
use serde_json::Value as JsonValue;
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

use specforge_core::{diff, lint, lint_config, parse_file, resolve, resolve_spec_path, scan_versions, DiffSeverity, LintConfig, RuleSeverity, Severity};

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
        Commands::Emit(args) => match run_emit(&args) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("error: {e:#}");
                let _ = warn!("{e:#}");
                ExitCode::FAILURE
            }
        },
        Commands::Init(args) => match run_init(&args) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("error: {e:#}");
                let _ = warn!("{e:#}");
                ExitCode::FAILURE
            }
        },
        Commands::Convert(args) => match run_convert(&args) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("error: {e:#}");
                let _ = warn!("{e:#}");
                ExitCode::FAILURE
            }
        },
        Commands::Docs(args) => match run_docs(&args) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("error: {e:#}");
                let _ = warn!("{e:#}");
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
                let _ = warn!("{e:#}");
                ExitCode::FAILURE
            }
        },
        Commands::Versions(args) => match run_versions(args) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("error: {e:#}");
                let _ = warn!("{e:#}");
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

    info!(
        "emitting {:?} SDK to: {}",
        cli.lang,
        cli.out.display()
    );

    let t2 = std::time::Instant::now();
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
    let emit_time = t2.elapsed();

    if written.is_empty() {
        bail!("emitter wrote zero files");
    }

    if cli.profile {
        eprintln!("Profile:");
        eprintln!("  parse:   {parse_time:?}");
        eprintln!("  resolve: {resolve_time:?}");
        eprintln!("  emit:    {emit_time:?}");
        eprintln!("  total:   {:?}", start.elapsed());
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

    let spec_path = cli.spec.as_ref().context("a spec file is required (or use --schema)")?;
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
        let json = serde_json::to_string_pretty(&doc)
            .context("failed to serialize IR to JSON")?;
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

fn run_init(cli: &InitArgs) -> Result<()> {
    std::fs::create_dir_all(&cli.out)
        .with_context(|| format!("failed to create output directory {}", cli.out.display()))?;

    let title_escaped = cli.title.replace('"', "\\\"");
    let dollar_ref = ["$", "ref"].concat();
    let schema_path = ["#", "/", "components", "/schemas", "/HealthResponse"].concat();
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

    info!("generating {:?} test file(s) to: {}", lang, cli.out.display());

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
    eprintln!("{:<width$}  {}", ver_header, file_header, width = max_ver_len.max(ver_header.len()));
    eprintln!("{:-<width$}  {:-<40}", "", "", width = max_ver_len.max(ver_header.len()));
    for info in &versions {
        eprintln!("{:<width$}  {}", info.version, info.path.display(), width = max_ver_len.max(ver_header.len()));
    }

    eprintln!("\n{} version(s) found", versions.len());
    Ok(())
}

fn run_convert(cli: &ConvertArgs) -> Result<()> {
    info!("reading spec: {}", cli.spec.display());
    let bytes = std::fs::read(&cli.spec)
        .with_context(|| format!("failed to read spec at {}", cli.spec.display()))?;
    let text = std::str::from_utf8(&bytes)
        .context("spec is not valid UTF-8")?;

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
                obj.insert("openapi".to_string(), JsonValue::String("3.1.0".to_string()));
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
                obj.insert("openapi".to_string(), JsonValue::String("3.0.3".to_string()));
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

/// Recursively upgrade OpenAPI 3.0 constructs to 3.1 equivalents.
fn upgrade_30_to_31(json: &mut JsonValue) {
    match json {
        JsonValue::Object(obj) => {
            // Convert `type: X` + `nullable: true` → `type: ["X", "null"]`
            if let (Some(type_val), Some(nullable)) = (obj.get("type").cloned(), obj.get("nullable").cloned()) {
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
                        let companion = if *field == "exclusiveMinimum" { "minimum" } else { "maximum" };
                        if let Some(limit) = obj.get(companion).cloned() {
                            if limit.is_number() {
                                obj.insert(field.to_string(), limit);
                            }
                        }
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

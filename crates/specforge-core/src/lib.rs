//! specforge-core
pub mod analyzer;
pub mod changelog;
pub mod dashboard;
pub mod demo;
pub mod deprecation;
pub mod diff;
pub mod docs;
pub mod error;
pub mod evolution;
pub mod graph;
pub mod i18n;
pub mod infer;
pub mod ir;
pub mod lint;
pub mod lint_config;
pub mod marketplace;
pub mod merge;
pub mod mock;
pub mod profiler;
pub mod resolve;
pub mod schema31;
pub mod security;
pub mod spec;
pub mod swagger_export;
pub mod testgen;
pub mod validate;
pub mod verify;
pub mod versioning;
pub mod workspace;

pub use analyzer::{analyze_spec, AnalysisReport};
pub use changelog::{
    generate_changelog, generate_changelog_result, ChangeImpact, ChangelogFormat, ChangelogOptions,
    ChangelogResult, OperationEntry, PropertyChangeEntry, SchemaChangeEntry, VersionBump,
};
pub use dashboard::generate_dashboard;
pub use demo::generate_demo_spec;
pub use deprecation::{
    find_deprecations, generate_migration_guide, DeprecationInfo, DeprecationKind,
};
pub use diff::{
    diff, diff_detailed, format_colored, format_json, format_markdown, format_text, DiffFinding,
    DiffFormat, DiffJsonOutput, DiffResult, DiffSeverity, DiffSummary, PropertyChange,
    PropertyChangeKind, SchemaDiffDetail,
};
pub use error::{ResolveError, SpecError};
pub use evolution::{
    format_json as evolution_format_json, format_markdown as evolution_format_markdown,
    format_text as evolution_format_text, track_evolution, EvolutionFormat, SchemaEvolution,
    VersionSnapshot,
};
pub use graph::{generate_graph, GraphFormat};
pub use i18n::I18nConfig;
pub use infer::{infer_openapi, infer_schema, InferOptions};
pub use ir::{
    Composition, CompositionKind, Discriminator, Document, EnumModel, EnumVariant, HttpMethod,
    Model, ObjectModel, Operation, ParamLocation, Parameter, Property, RequestBody, Response,
    Scalar, SchemaRegistry, SecurityScheme, Type, Webhook, IR_VERSION,
};
pub use lint::{lint_swagger_editor, Diagnostic, Severity};
pub use lint_config::{LintConfig, LintRule, RuleSeverity};
pub use marketplace::{MarketplaceIndex, PluginEntry, PluginIndex, SpecEntry};
pub use merge::merge_specs;
pub use mock::MockServer;
pub use profiler::{profile_api, ProfileOptions, ProfileReport, ProfileResult};
pub use resolve::{resolve, resolve_with_webhooks};
pub use security::{
    analyze_security, analyze_security_detailed, OperationSecurity, SecurityIssue, SecurityReport,
    SecuritySchemeInfo,
};
pub use spec::{
    detect_31_features, parse_bytes, parse_bytes_full, parse_file, parse_file_full, parse_str,
    parse_str_full, resolve_spec_path, scan_versions, ParsedSpec, Spec31Features, VersionInfo,
};
pub use swagger_export::{
    export_spec, export_swagger_editor, ExportError, ExportFormat, ExportOptions,
};
pub use testgen::{generate_tests, TestGenOptions, TestLang};
pub use validate::{validate, ValidationError};
pub use verify::{verify_api, VerifyOptions, VerifyResult};
pub use versioning::{apply_versioning, VersionStrategy, VersioningConfig};
pub use workspace::{
    init_workspace, PluginConfig, SpecforgeConfig, WorkspaceConfig, WorkspaceInitResult,
    WorkspaceOutput, WorkspaceRunResult, WorkspaceSpec,
};

//! `specforge-core` — OpenAPI spec parsing, `$ref` resolution, and a
//! language-neutral IR for SDK code generation.
//!
//! Pipeline: raw bytes → [`spec::parse_bytes`] → `openapiv3::OpenAPI` →
//! [`resolve::resolve`] → [`ir::Document`] (the IR emitters consume).
//!
//! The IR is the only thing downstream emitters are allowed to see. They never
//! import `openapiv3`, so swapping the parser later won't ripple into emitters.

pub mod deprecation;
pub mod diff;
pub mod docs;
pub mod error;
pub mod ir;
pub mod lint;
pub mod lint_config;
pub mod merge;
pub mod resolve;
pub mod spec;
pub mod testgen;
pub mod validate;
pub mod workspace;

pub use deprecation::{find_deprecations, generate_migration_guide, DeprecationInfo, DeprecationKind};
pub use diff::{diff, DiffFinding, DiffSeverity};
pub use error::{ResolveError, SpecError};
pub use merge::merge_specs;
pub use testgen::{generate_tests, TestGenOptions, TestLang};
pub use ir::{
    Composition, CompositionKind, Discriminator, Document, EnumModel, EnumVariant, HttpMethod,
    Model, ObjectModel, Operation, Parameter, ParamLocation, Property, RequestBody, Response,
    Scalar, SchemaRegistry, SecurityScheme, Type, Webhook,
};
pub use lint::{Diagnostic, Severity};
pub use lint_config::{LintConfig, LintRule, RuleSeverity};
pub use resolve::{resolve, resolve_with_webhooks};
pub use spec::{detect_31_features, parse_bytes, parse_bytes_full, parse_file, parse_file_full, parse_str, parse_str_full, resolve_spec_path, scan_versions, ParsedSpec, Spec31Features, VersionInfo};
pub use validate::{validate, ValidationError};
pub use workspace::{init_workspace, WorkspaceConfig, WorkspaceInitResult, WorkspaceOutput, WorkspaceRunResult, WorkspaceSpec};

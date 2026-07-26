//! `specforge-core` — OpenAPI spec parsing, `$ref` resolution, and a
//! language-neutral IR for SDK code generation.
//!
//! Pipeline: raw bytes → [`spec::parse_bytes`] → `openapiv3::OpenAPI` →
//! [`resolve::resolve`] → [`ir::Document`] (the IR emitters consume).
//!
//! The IR is the only thing downstream emitters are allowed to see. They never
//! import `openapiv3`, so swapping the parser later won't ripple into emitters.

pub mod error;
pub mod ir;
pub mod resolve;
pub mod spec;

pub use error::{ResolveError, SpecError};
pub use ir::{
    Composition, CompositionKind, Discriminator, Document, EnumModel, EnumVariant, HttpMethod,
    Model, ObjectModel, Operation, Parameter, ParamLocation, Property, RequestBody, Response,
    Scalar, SchemaRegistry, SecurityScheme, Type,
};
pub use resolve::resolve;
pub use spec::{parse_bytes, parse_file, parse_str};

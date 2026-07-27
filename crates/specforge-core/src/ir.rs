//! Language-neutral intermediate representation (IR).
//!
//! The IR decouples *understanding the OpenAPI spec* (parsing + resolution,
//! which is language-agnostic) from *emitting a target language* (TypeScript
//! here, Rust/Go/Python later). Each emitter walks the IR; none of them ever
//! touches `openapiv3` types directly.
//!
//! Design notes:
//! - All `$ref` targets are resolved to plain string names by the resolver
//!   before IR construction, so emitters never resolve references themselves.
//! - Composition (`allOf`/`oneOf`/`anyOf`) is preserved as a node variant so
//!   each emitter can map it idiomatically (TS intersections/unions).

use indexmap::IndexMap;

// ─── Schema IR ──────────────────────────────────────────────────────────────

/// A resolved schema, ready for emission. `Reference` is the only place a
/// named model is mentioned — everything else is structural.
#[derive(Debug, Clone, serde::Serialize)]
pub enum Type {
    /// A primitive scalar.
    Scalar(Scalar),
    /// A closed set of string literals (inline `type: string, enum: [...]` on a
    /// property, or a one-value discriminant). Named enums still use
    /// [`Model::Enum`] + [`Type::Reference`].
    StringEnum {
        variants: Vec<String>,
        nullable: bool,
    },
    /// An array of `item`.
    Array {
        item: Box<Type>,
        nullable: bool,
    },
    /// A map with arbitrary string keys (OpenAPI `additionalProperties`).
    Map {
        value: Box<Type>,
    },
    /// A reference to a named model in the [`SchemaRegistry`].
    Reference {
        name: String,
        nullable: bool,
        /// Optional description override from a `$ref` sibling (OpenAPI 3.1).
        /// When present, emitters should prefer this over the referenced
        /// model's own description.
        description: Option<String>,
    },
    /// A composition of other types.
    Composition(Composition),
    /// Untyped / empty schema (`{}`). Emits to the target's "any" equivalent.
    Any,
    /// Unknown — used as a fallback for malformed schemas. Emits to `unknown`.
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum Scalar {
    String,
    /// `format: date` or `format: date-time`. Emits to the target's date type.
    DateTime,
    /// `format: uuid`.
    Uuid,
    Integer,
    /// `format: int64` where the target can't represent it losslessly.
    Integer64,
    Float,
    Boolean,
}

/// OpenAPI `discriminator` metadata for a `oneOf`/`anyOf` composition.
/// Emitters use this to generate runtime type guards that narrow union arms.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Discriminator {
    /// Property name used to distinguish variants (e.g. `"type"`).
    pub property_name: String,
    /// Explicit mapping from discriminator values to schema names.
    /// When present, emitters use this instead of inferring values from
    /// single-variant string enums on each arm.
    /// Keys are discriminant values (e.g. `"dog"`), values are schema names
    /// (e.g. `"Dog"` — the trailing `#/components/schemas/` is stripped).
    pub mapping: Option<IndexMap<String, String>>,
}

/// A composition: `allOf` → intersection, `oneOf`/`anyOf` → union.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Composition {
    pub kind: CompositionKind,
    pub members: Vec<Type>,
    /// Present when the OpenAPI schema declared a `discriminator`.
    pub discriminator: Option<Discriminator>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum CompositionKind {
    /// `allOf` — emitted as an intersection.
    AllOf,
    /// `oneOf` — emitted as a union.
    OneOf,
    /// `anyOf` — emitted as a union.
    AnyOf,
}

/// A named model (the concrete backing of a `Reference`). Despite the name,
/// the schema it came from may not have been a true `object` — it could be a
/// composition (`oneOf`/`allOf`/`anyOf`) or a scalar alias. In those cases
/// `properties` is empty and `shape_type` carries the underlying type, so an
/// emitter can render a `type` alias instead of an `interface`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ObjectModel {
    pub name: String,
    pub description: Option<String>,
    pub properties: Vec<Property>,
    pub additional_properties: Option<Box<Type>>,
    /// The full underlying type of this schema. For plain objects this mirrors
    /// the properties; for compositions/scalars it holds the actual shape.
    pub shape_type: Option<Type>,
    /// When `allOf` has exactly one `$ref` member, this records that base type
    /// so emitters can use embedding (Go) or flatten (Rust) instead of merging
    /// all properties into a flat struct.
    pub base_type: Option<Type>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Property {
    pub name: String,
    pub ty: Type,
    pub required: bool,
    pub description: Option<String>,
}

/// A string enum (the only enum shape OpenAPI commonly uses for models).
#[derive(Debug, Clone, serde::Serialize)]
pub struct EnumModel {
    pub name: String,
    pub description: Option<String>,
    pub variants: Vec<EnumVariant>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct EnumVariant {
    /// The raw enum value from the spec.
    pub value: String,
    pub description: Option<String>,
}

/// Anything that can be referenced by name and emitted as its own unit.
#[derive(Debug, Clone, serde::Serialize)]
pub enum Model {
    Object(ObjectModel),
    Enum(EnumModel),
}

impl Model {
    pub fn name(&self) -> &str {
        match self {
            Model::Object(o) => &o.name,
            Model::Enum(e) => &e.name,
        }
    }
}

/// Ordered collection of named models. Ordering is preserved from the spec so
/// generated output is deterministic across runs.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct SchemaRegistry {
    pub models: IndexMap<String, Model>,
}

impl SchemaRegistry {
    pub fn get(&self, name: &str) -> Option<&Model> {
        self.models.get(name)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&String, &Model)> {
        self.models.iter()
    }
}

// ─── Operation IR ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize)]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Patch,
    Delete,
    Head,
    Options,
}

impl HttpMethod {
    pub fn as_str(self) -> &'static str {
        match self {
            HttpMethod::Get => "get",
            HttpMethod::Post => "post",
            HttpMethod::Put => "put",
            HttpMethod::Patch => "patch",
            HttpMethod::Delete => "delete",
            HttpMethod::Head => "head",
            HttpMethod::Options => "options",
        }
    }

    /// Uppercased method name, suitable for an HTTP client.
    pub fn upper(self) -> &'static str {
        match self {
            HttpMethod::Get => "GET",
            HttpMethod::Post => "POST",
            HttpMethod::Put => "PUT",
            HttpMethod::Patch => "PATCH",
            HttpMethod::Delete => "DELETE",
            HttpMethod::Head => "HEAD",
            HttpMethod::Options => "OPTIONS",
        }
    }
}

/// Where a parameter lives in the request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum ParamLocation {
    Path,
    Query,
    Header,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Parameter {
    pub name: String,
    pub location: ParamLocation,
    pub ty: Type,
    pub required: bool,
    pub description: Option<String>,
}

/// A single response variant for an operation.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Response {
    /// Status code, or `*` for a default response. Kept as a string so ranges
    /// (e.g. `"2XX"`) survive without extra modeling for v1.
    pub status: String,
    pub description: Option<String>,
    pub body: Option<Type>,
}

/// The resolved body schema for a request or response, if any.
#[derive(Debug, Clone, serde::Serialize)]
pub struct RequestBody {
    pub ty: Type,
    pub required: bool,
    pub description: Option<String>,
}

/// A fully-resolved operation ready for emission.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Operation {
    pub operation_id: String,
    pub method: HttpMethod,
    pub path: String,
    pub tag: Option<String>,
    pub summary: Option<String>,
    pub description: Option<String>,
    pub parameters: Vec<Parameter>,
    pub request_body: Option<RequestBody>,
    pub responses: Vec<Response>,
}

/// A resolved webhook ready for emission. Webhooks are OpenAPI 3.1 callbacks
/// that the API can invoke on the client. Each webhook has a name (the map key
/// in the spec), a method, and request/response definitions.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Webhook {
    /// The webhook name from the spec (e.g. `newPet`).
    pub name: String,
    /// The HTTP method (typically POST).
    pub method: HttpMethod,
    /// A synthetic path for reference (e.g. `/webhooks/newPet`).
    pub path: String,
    pub summary: Option<String>,
    pub description: Option<String>,
    pub request_body: Option<RequestBody>,
    pub responses: Vec<Response>,
}

// ─── Security IR ────────────────────────────────────────────────────────────

/// Authentication schemes the SDK's runtime needs to support.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub enum SecurityScheme {
    HttpBearer,
    ApiKey { header: String },
}

// ─── Top-level IR ───────────────────────────────────────────────────────────

/// The complete, resolved IR for a document. This is what emitters consume.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Document {
    pub title: String,
    pub version: String,
    pub base_url: Option<String>,
    pub security: Vec<SecurityScheme>,
    pub schemas: SchemaRegistry,
    pub operations: Vec<Operation>,
    pub webhooks: Vec<Webhook>,
}

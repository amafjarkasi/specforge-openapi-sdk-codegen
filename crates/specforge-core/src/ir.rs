use indexmap::IndexMap;

#[derive(Debug, Clone, serde::Serialize)]
pub enum Type { Scalar(Scalar), StringEnum { variants: Vec<String>, nullable: bool }, Array { item: Box<Type>, nullable: bool }, Map { value: Box<Type> }, Reference { name: String, nullable: bool, description: Option<String> }, Composition(Composition), Any, Unknown }

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum Scalar { String, DateTime, Uuid, Integer, Integer64, Float, Boolean, Base64, Binary }

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Discriminator { pub property_name: String, pub mapping: Option<IndexMap<String, String>> }

#[derive(Debug, Clone, serde::Serialize)]
pub struct Composition { pub kind: CompositionKind, pub members: Vec<Type>, pub discriminator: Option<Discriminator> }

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum CompositionKind { AllOf, OneOf, AnyOf }

#[derive(Debug, Clone, serde::Serialize)]
pub struct ObjectModel { pub name: String, pub description: Option<String>, pub properties: Vec<Property>, pub additional_properties: Option<Box<Type>>, pub shape_type: Option<Type>, pub base_type: Option<Type> }

#[derive(Debug, Clone, serde::Serialize)]
pub struct Property { pub name: String, pub ty: Type, pub required: bool, pub description: Option<String> }

#[derive(Debug, Clone, serde::Serialize)]
pub struct EnumModel { pub name: String, pub description: Option<String>, pub variants: Vec<EnumVariant> }

#[derive(Debug, Clone, serde::Serialize)]
pub struct EnumVariant { pub value: String, pub description: Option<String> }

#[derive(Debug, Clone, serde::Serialize)]
pub enum Model { Object(ObjectModel), Enum(EnumModel) }
impl Model { pub fn name(&self) -> &str { match self { Model::Object(o) => &o.name, Model::Enum(e) => &e.name } } }

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct SchemaRegistry { pub models: IndexMap<String, Model> }
impl SchemaRegistry { pub fn get(&self, name: &str) -> Option<&Model> { self.models.get(name) } pub fn iter(&self) -> impl Iterator<Item = (&String, &Model)> { self.models.iter() } }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize)]
pub enum HttpMethod { Get, Post, Put, Patch, Delete, Head, Options }
impl HttpMethod { pub fn as_str(self) -> &'static str { match self { HttpMethod::Get => "get", HttpMethod::Post => "post", HttpMethod::Put => "put", HttpMethod::Patch => "patch", HttpMethod::Delete => "delete", HttpMethod::Head => "head", HttpMethod::Options => "options" } } pub fn upper(self) -> &'static str { match self { HttpMethod::Get => "GET", HttpMethod::Post => "POST", HttpMethod::Put => "PUT", HttpMethod::Patch => "PATCH", HttpMethod::Delete => "DELETE", HttpMethod::Head => "HEAD", HttpMethod::Options => "OPTIONS" } } }

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum ParamLocation { Path, Query, Header }

#[derive(Debug, Clone, serde::Serialize)]
pub struct Parameter { pub name: String, pub location: ParamLocation, pub ty: Type, pub required: bool, pub description: Option<String> }

#[derive(Debug, Clone, serde::Serialize)]
pub struct Response { pub status: String, pub description: Option<String>, pub body: Option<Type> }

#[derive(Debug, Clone, serde::Serialize)]
pub struct RequestBody { pub ty: Type, pub required: bool, pub description: Option<String> }

#[derive(Debug, Clone, serde::Serialize)]
pub struct RetryPolicy { pub max_retries: Option<u32>, pub retryable: bool }

#[derive(Debug, Clone, serde::Serialize)]
pub struct Operation { pub operation_id: String, pub method: HttpMethod, pub path: String, pub tag: Option<String>, pub summary: Option<String>, pub description: Option<String>, pub parameters: Vec<Parameter>, pub request_body: Option<RequestBody>, pub responses: Vec<Response>, pub retry_policy: Option<RetryPolicy> }

#[derive(Debug, Clone, serde::Serialize)]
pub struct Webhook { pub name: String, pub method: HttpMethod, pub path: String, pub summary: Option<String>, pub description: Option<String>, pub request_body: Option<RequestBody>, pub responses: Vec<Response> }

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub enum SecurityScheme { HttpBearer, ApiKey { header: String } }

/// The current IR schema version. Increment this when breaking IR changes are made.
pub const IR_VERSION: &str = "1.0";

#[derive(Debug, Clone, serde::Serialize)]
pub struct Document {
    /// IR schema version for compatibility checks.
    pub ir_version: String,
    pub title: String,
    pub version: String,
    pub base_url: Option<String>,
    pub security: Vec<SecurityScheme>,
    pub schemas: SchemaRegistry,
    pub operations: Vec<Operation>,
    pub webhooks: Vec<Webhook>,
}

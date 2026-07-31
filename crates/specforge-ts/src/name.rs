//! Identifier sanitization for TypeScript emission.
//!
//! OpenAPI schema/property names can be arbitrary strings; TypeScript
//! identifiers can't. We keep the original spelling where it's already valid,
//! and otherwise transform it deterministically so two runs over the same spec
//! always produce identical output.

/// Sanitize a name into a valid TS type identifier (PascalCase for types).
///
/// Rules:
/// - Non-alphanumeric characters become word boundaries (`_`, `-`, `.`).
/// - Each word is title-cased and concatenated.
/// - Names starting with a digit are prefixed with `_`.
/// - The empty result becomes `_`.
pub fn pascal(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut next_upper = true;
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() {
            if next_upper {
                out.extend(ch.to_uppercase());
                next_upper = false;
            } else {
                out.push(ch);
            }
        } else {
            next_upper = true;
        }
    }
    if out.is_empty() {
        return "_".to_string();
    }
    if out.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        out.insert(0, '_');
    }
    out
}

/// Whether `name` matches a type the TypeScript SDK emitter hardcodes into the
/// generated package (ApiClient, ResponseCache, ValidationError, etc.). User
/// models with these names are suffixed with `Model` to avoid duplicate-export
/// / redeclaration errors. Only applied to generated model/enum names, not to
/// all pascal-case identifiers.
pub fn is_ts_sdk_builtin_type(name: &str) -> bool {
    matches!(
        name,
        "ApiClient"
            | "ApiClientOptions"
            | "ApiError"
            | "AuthProvider"
            | "AnonymousAuthProvider"
            | "BearerAuthProvider"
            | "ApiKeyAuthProvider"
            | "CacheEntry"
            | "ConfigurationError"
            | "ConsoleLogger"
            | "CursorPage"
            | "EndpointSchema"
            | "HttpError"
            | "Logger"
            | "Metrics"
            | "MetricsCollector"
            | "Middleware"
            | "MiddlewareRequest"
            | "MiddlewareResponse"
            | "NetworkError"
            | "OffsetPage"
            | "RateLimiter"
            | "RequestInterceptor"
            | "RequestDeduper"
            | "RequestOptions"
            | "ResponseCache"
            | "ResponseInterceptor"
            | "ResponseTransformer"
            | "RetryOptions"
            | "RouteSchemaMap"
            | "SchemaType"
            | "Semaphore"
            | "ServerSentEvent"
            | "ServiceContainer"
            | "SlidingWindow"
            | "TelemetryHooks"
            | "TimeoutError"
            | "TokenBucket"
            | "ValidationError"
            | "QueryRecord"
    )
}

/// Wrap a generated model/enum name so it can't collide with a built-in SDK
/// type. Returns `name` unchanged when safe, or `name + "Model"` when it would.
pub fn safe_model_name(name: &str) -> String {
    if is_ts_sdk_builtin_type(name) {
        format!("{name}Model")
    } else {
        name.to_string()
    }
}

/// Sanitize a name into a valid TS *value* identifier (camelCase).
pub fn camel(input: &str) -> String {
    let pascal = pascal(input);
    let mut chars = pascal.chars();
    let first = chars.next().map(|c| c.to_ascii_lowercase()).unwrap_or_default();
    format!("{first}{}", chars.as_str())
}

/// Quote a property name if it isn't a valid bare identifier (e.g. `kebab-case`,
/// `with spaces`, leading digit). Used when rendering object/interface members.
pub fn property_key(name: &str) -> String {
    if is_valid_identifier(name) && !is_reserved(name) {
        name.to_string()
    } else {
        format!("\"{}\"", escape_quotes(name))
    }
}

/// Render a member-access expression `obj.name`. Uses dot notation for valid
/// identifiers and bracket notation otherwise (e.g. `obj["package"]` for
/// reserved words, `obj["kebab-case"]` for hyphenated names).
pub fn member_access(obj: &str, name: &str) -> String {
    if is_valid_identifier(name) && !is_reserved(name) {
        format!("{obj}.{name}")
    } else {
        format!("{obj}[\"{}\"]", escape_quotes(name))
    }
}

/// Quote a string literal for emission inside a TS string-union enum.
pub fn string_literal(value: &str) -> String {
    format!("\"{}\"", escape_quotes(value))
}

fn escape_quotes(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

fn is_valid_identifier(s: &str) -> bool {
    let mut chars = s.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first.is_ascii_alphabetic() || first == '_' || first == '$')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
}

fn is_reserved(s: &str) -> bool {
    matches!(
        s,
        "break" | "case" | "catch" | "class" | "const" | "continue" | "debugger"
            | "default" | "delete" | "do" | "else" | "enum" | "export" | "extends"
            | "false" | "finally" | "for" | "function" | "if" | "import" | "in"
            | "instanceof" | "new" | "null" | "return" | "super" | "switch" | "this"
            | "throw" | "true" | "try" | "typeof" | "var" | "void" | "while" | "with"
            | "let" | "static" | "yield" | "await" | "async" | "of" | "as" | "from"
            | "type" | "interface" | "implements" | "package" | "private" | "protected"
            | "public"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pascal_casifies() {
        assert_eq!(pascal("pet"), "Pet");
        assert_eq!(pascal("pet-event"), "PetEvent");
        assert_eq!(pascal("PetCreated"), "PetCreated");
        assert_eq!(pascal("my_cool_field"), "MyCoolField");
    }

    #[test]
    fn pascal_handles_digits_and_empty() {
        assert_eq!(pascal("123abc"), "_123abc");
        assert_eq!(pascal(""), "_");
        assert_eq!(pascal("a.b.c"), "ABC");
    }

    #[test]
    fn camel_lowercases_first() {
        assert_eq!(camel("PetEvent"), "petEvent");
        assert_eq!(camel("list-pets"), "listPets");
    }

    #[test]
    fn property_keys_get_quoted_when_needed() {
        assert_eq!(property_key("name"), "name");
        assert_eq!(property_key("kebab-case"), "\"kebab-case\"");
        assert_eq!(property_key("with space"), "\"with space\"");
        assert_eq!(property_key("default"), "\"default\"");
    }

    #[test]
    fn string_literals_escape() {
        assert_eq!(string_literal("a\"b"), "\"a\\\"b\"");
        assert_eq!(string_literal("dog"), "\"dog\"");
    }
}

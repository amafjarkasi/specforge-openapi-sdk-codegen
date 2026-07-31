# Security Policy

## Supported versions

| Version | Supported |
|---------|-----------|
| 1.6.x   | ✅        |
| < 1.6   | ❌        |

## Reporting a vulnerability

**Do not open a public issue.** Instead, email the maintainers via the contact form at [specforge.deepwhaleai.com](https://specforge.deepwhaleai.com), or open a private security advisory on GitHub (Security → Advisories → New draft security advisory).

Please include:

- A description of the vulnerability and its potential impact
- Steps to reproduce (a minimal OpenAPI spec that triggers it is ideal)
- The affected crate/component (e.g. `specforge-core` parser, `specforge-go` emitter)

We'll respond within 7 days and aim to ship a fix within 30 days of confirmation.

## Scope

specforge processes user-supplied OpenAPI specs (YAML/JSON) and writes generated code to disk. The following are in-scope for security reports:

- **Parser panics or denial of service** — a malformed spec that crashes or hangs the library
- **Path traversal** — generated file paths that escape the user-specified output directory
- **Injection in generated code** — spec content that breaks out of string literals or doc comments in generated SDKs
- **Memory safety** — any `unsafe` code issues (there are very few `unsafe` blocks; the primary one is in `specforge-plugin` WASM FFI)

Out of scope: issues in generated SDKs' runtime behavior (those are the consumer's responsibility to review), or issues in the VS Code extension's network calls.

## Credits

We'll credit reporters in the changelog (with permission).

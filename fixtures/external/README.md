# External OpenAPI Specs for Testing

Real-world OpenAPI specs downloaded from public sources. Used to verify specforge
handles a range of spec sizes, schema counts, and OpenAPI features.

All specs are OpenAPI 3.x and have been verified to pass `specforge generate`.

## Spec Inventory

### Small (< 50 schemas)

| File | Source | Version | Schemas | Operations | Size |
|------|--------|---------|---------|------------|------|
| `petstore.json` | [Swagger Petstore](https://petstore3.swagger.io/api/v3/openapi.json) | 3.0.4 | 6 | 19 | 17 KB |
| `twilio-accounts.yaml` | [APIs-guru / Twilio](https://github.com/APIs-guru/openapi-directory/tree/main/APIs/twilio.com/twilio_accounts_v1) | 3.0.1 | 6 | 16 | 26 KB |

**What they test:** Basic CRUD operations, simple schemas, inline request/response bodies,
enum values, path parameters, query parameters.

### Medium (50-200 schemas)

| File | Source | Version | Schemas | Operations | Size |
|------|--------|---------|---------|------------|------|
| `kubernetes.json` | [Kubernetes apps/v1](https://github.com/kubernetes/kubernetes/tree/master/api/openapi-spec/v3) | 3.0.0 | 166 | 77 | 935 KB |
| `twilio-api.yaml` | [APIs-guru / Twilio](https://github.com/APIs-guru/openapi-directory/tree/main/APIs/twilio.com/api) | 3.0.1 | 149 | 195 | 1.1 MB |

**What they test:** Deep schema composition (`allOf`, `oneOf`), nested object references,
large tag groupings, multiple content types, `x-` vendor extensions.

### Large (200+ schemas)

| File | Source | Version | Schemas | Operations | Size |
|------|--------|---------|---------|------------|------|
| `github.yaml` | [GitHub REST API](https://github.com/APIs-guru/openapi-directory/tree/main/APIs/github.com/api.github.com/1.1.4) | 3.0.3 | 582 | 845 | 8.8 MB |
| `stripe.yaml` | [Stripe API](https://github.com/stripe/openapi/blob/master/openapi/spec3.yaml) | 3.0.0 | 1,431 | 587 | 6.3 MB |

**What they test:** Very large schemas with many `allOf`/`oneOf`/`anyOf` compositions,
discriminator mappings, circular `$ref` chains, nullable fields, polymorphism,
security schemes (OAuth2, API keys), webhook definitions, pagination patterns.

## Download Sources

All specs were fetched from:

- **Swagger Petstore:** `https://petstore3.swagger.io/api/v3/openapi.json`
- **APIs-guru (openapi-directory):** `https://raw.githubusercontent.com/APIs-guru/openapi-directory/main/APIs/...`
- **GitHub REST API:** Same as above, via the `api.github.com` entry
- **Stripe:** `https://raw.githubusercontent.com/stripe/openapi/master/openapi/spec3.yaml`
- **Kubernetes:** `https://raw.githubusercontent.com/kubernetes/kubernetes/master/api/openapi-spec/v3/apis__apps__v1_openapi.json`

## Usage

```bash
# Validate a spec
specforge check fixtures/external/petstore.json

# Generate TypeScript SDK
specforge generate fixtures/external/stripe.yaml --lang ts -o /tmp/stripe-sdk

# Generate Go SDK
specforge generate fixtures/external/github.yaml --lang go -o /tmp/github-go

# Generate Rust SDK
specforge generate fixtures/external/kubernetes.json --lang rust -o /tmp/k8s-rust
```

## Verification

All specs pass `specforge generate` across all three target languages (ts, go, rust):

```
petstore.json        -> 30 files (ts)
twilio-accounts.yaml -> 32 files (ts)
kubernetes.json      -> 188 files (ts)
twilio-api.yaml      -> 236 files (ts)
stripe.yaml          -> 1,453 files (ts)
github.yaml          -> 635 files (ts)
```

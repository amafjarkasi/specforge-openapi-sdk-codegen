---
title: Examples
sidebar_position: 1
description: Real-world usage examples
---


# Examples

Minimal consumer stubs that call SDKs generated from `fixtures/petstore.yaml`.

```bash
# From repo root — regenerate sdk/ folders after emitter changes
./scripts/generate-examples.sh
```

| Directory | Language | How to run |
|-----------|----------|------------|
| [`petstore-ts/`](https://github.com/amafjarkasi/specforge-openapi-sdk-codegen/tree/master/examples/petstore-ts) | TypeScript | `cd petstore-ts && npm i && npx tsx main.mts` |
| [`petstore-go/`](https://github.com/amafjarkasi/specforge-openapi-sdk-codegen/tree/master/examples/petstore-go) | Go | `cd petstore-go && go run .` |
| [`petstore-rust/`](https://github.com/amafjarkasi/specforge-openapi-sdk-codegen/tree/master/examples/petstore-rust) | Rust | `cd petstore-rust && cargo run` |

> These examples expect a live Petstore at the URL in each file (default mock or public).  
> For offline demos, point `baseUrl` at the e2e mock or your own server.

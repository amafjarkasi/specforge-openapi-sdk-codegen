# specforge for VS Code

Generate typed SDKs from OpenAPI specs directly in VS Code.

## Features

- **Generate SDK** — Generate TypeScript, Go, or Rust SDK from your OpenAPI spec
- **Check Spec** — Lint and validate your OpenAPI spec
- **Preview IR** — View the resolved intermediate representation as JSON

## Requirements

- `specforge` binary installed and on PATH (or configure `specforge.binaryPath`)

## Extension Settings

- `specforge.defaultLang` — Default target language (ts/go/rust)
- `specforge.outputDir` — Default output directory
- `specforge.binaryPath` — Path to specforge binary

## Usage

1. Open a workspace with an OpenAPI spec (openapi.yaml or openapi.json)
2. Run `Ctrl+Shift+P` → "specforge: Generate SDK"
3. Generated SDK appears in the output directory

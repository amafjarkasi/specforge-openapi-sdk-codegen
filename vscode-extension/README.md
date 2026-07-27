# specforge for VS Code

Generate, lint, diff, merge, and manage typed SDKs from OpenAPI specs directly in VS Code.

## Features

- **24 commands** covering the full specforge CLI workflow
- Interactive language, version, and format pickers
- Auto-validation on save (optional)
- Output channel with full CLI logs
- Status bar indicator for mock server
- Context menus, keybindings, and explorer integration

## Requirements

- `specforge` binary installed and on PATH, or configure `specforge.binaryPath`

## Commands

| Command | Shortcut | Description |
|---------|----------|-------------|
| `specforge: Generate SDK` | `Ctrl+Shift+G` | Generate a typed SDK (TypeScript, Go, or Rust) from an OpenAPI spec |
| `specforge: Lint & Validate Spec` | `Ctrl+Shift+V` | Lint and validate an OpenAPI spec (normal or strict mode) |
| `specforge: Diff Two Specs` | -- | Compare two specs and report breaking changes |
| `specforge: Preview IR as JSON` | `Ctrl+Shift+I` | Emit the resolved intermediate representation as formatted JSON |
| `specforge: Scaffold New Spec` | -- | Create a minimal OpenAPI spec with an example endpoint |
| `specforge: Convert Spec Version (3.0/3.1)` | -- | Convert a spec between OpenAPI 3.0 and 3.1 |
| `specforge: Merge Specs` | -- | Merge multiple OpenAPI spec files into one (YAML or JSON) |
| `specforge: Generate Migration Guide` | -- | Compare two spec versions and generate a migration guide |
| `specforge: Generate HTML Documentation` | -- | Generate a static HTML documentation site from a spec |
| `specforge: Generate Tests` | -- | Generate SDK test files with mock servers |
| `specforge: List API Versions` | -- | List all API versions found in a spec file or directory |
| `specforge: Generate Workspace SDKs` | -- | Generate SDKs for all specs defined in a workspace config |
| `specforge: Initialize Workspace Config` | -- | Scan a directory for spec files and create a workspace config |
| `specforge: Open Metrics Dashboard` | -- | Open the specforge metrics dashboard in a browser |
| `specforge: Analyze Security Requirements` | -- | Analyze authentication and authorization requirements in a spec |
| `specforge: Show Dependency Graph` | -- | Display the schema dependency graph |
| `specforge: Bundle Analysis` | -- | Analyze a spec for redundancy, unused schemas, and size issues |
| `specforge: Start Mock Server` | -- | Start a local mock HTTP server from a spec's example responses |
| `specforge: Export for Swagger Editor` | -- | Export a spec as a Swagger Editor-compatible bundle |
| `specforge: Generate Demo Spec` | -- | Generate a working demo Petstore spec with realistic examples |
| `specforge: Track Schema Evolution` | -- | Show how a schema has evolved across git commits |
| `specforge: Infer Spec from JSON` | -- | Infer an OpenAPI spec from a sample JSON request/response body |
| `specforge: Verify Running API` | -- | Hit endpoints on a running API and verify they match the spec |
| `specforge: Generate Changelog` | -- | Generate a CHANGELOG.md from an OpenAPI spec |

## Extension Settings

| Setting | Type | Default | Description |
|---------|------|---------|-------------|
| `specforge.binaryPath` | string | `"specforge"` | Path to the specforge binary. Leave as `specforge` if it is on your PATH. |
| `specforge.defaultLang` | string | `"ts"` | Default target language for SDK generation. Options: `ts`, `go`, `rust`. |
| `specforge.outputDir` | string | `"./generated"` | Default output directory for generated SDKs. |
| `specforge.autoValidate` | boolean | `false` | Automatically lint and validate OpenAPI specs when saved. |
| `specforge.specFilePattern` | string | `"**/openapi.{yaml,json},**/swagger.{yaml,json}"` | Glob pattern for finding OpenAPI spec files in the workspace. |
| `specforge.logLevel` | string | `"info"` | Log verbosity for specforge CLI commands. Options: `error`, `warn`, `info`, `debug`, `trace`. |

## Keybindings

| Shortcut | Command | Platform |
|----------|---------|----------|
| `Ctrl+Shift+G` | Generate SDK | Windows/Linux |
| `Cmd+Shift+G` | Generate SDK | macOS |
| `Ctrl+Shift+V` | Lint & Validate | Windows/Linux |
| `Cmd+Shift+V` | Lint & Validate | macOS |
| `Ctrl+Shift+I` | Preview IR | Windows/Linux |
| `Cmd+Shift+I` | Preview IR | macOS |

## Context Menus

- **Editor title bar**: Generate SDK and Lint buttons appear for YAML/JSON files
- **Editor context menu**: Generate, Lint, and Preview IR for YAML/JSON files
- **Explorer context menu**: Generate, Lint, Preview IR, Analyze, and Export for YAML/JSON files

## Usage

1. Open a workspace containing an OpenAPI spec (`openapi.yaml`, `openapi.json`, `swagger.yaml`, or `swagger.json`)
2. Open the Command Palette (`Ctrl+Shift+P` / `Cmd+Shift+P`)
3. Type `specforge:` to see all available commands
4. Select a command -- the extension will guide you through interactive pickers as needed

### Quick Start Examples

**Generate a TypeScript SDK:**
1. Open your `openapi.yaml` file
2. Run `specforge: Generate SDK`
3. Select "TypeScript" when prompted
4. Enter the output directory (default: `./generated`)

**Lint a spec:**
1. Open your spec file
2. Run `specforge: Lint & Validate Spec` (or press `Ctrl+Shift+V`)
3. Choose Normal or Strict mode

**Start a mock server:**
1. Run `specforge: Start Mock Server`
2. Enter a port (or leave empty for a random port)
3. The mock server starts and appears in the status bar

**Auto-validate on save:**
1. Enable `specforge.autoValidate` in settings
2. Every time you save an OpenAPI spec file, it will be linted automatically

## License

MIT

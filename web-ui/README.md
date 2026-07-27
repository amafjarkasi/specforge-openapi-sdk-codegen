# specforge Web UI

A browser-based previewer for the specforge IR.

## Usage

1. Generate the IR: `specforge emit openapi.yaml > ir.json`
2. Open `index.html` in a browser
3. Paste the JSON or upload `ir.json`
4. Browse schemas, operations, and the full IR

## Features

- Schema tree browser
- Operation listing
- IR JSON viewer
- Download IR as JSON
- Dark forge-themed UI

## Development

No build step needed — open `index.html` directly. All CSS and JS are inline.

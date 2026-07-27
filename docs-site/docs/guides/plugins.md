---
title: Plugins
sidebar_position: 1
description: specforge plugin system and WASM extensions
---


# specforge Plugin System

## Overview

specforge supports external emitters via WASM plugins. A plugin receives the resolved IR as JSON and returns generated files.

## Building a Plugin

1. Create a new Rust crate with `crate-type = ["cdylib"]`
2. Add `specforge-plugin` as a dependency
3. Implement the `Plugin` trait
4. Export using `specforge_plugin::export_plugin!`
5. Build with `cargo build --target wasm32-wasi --release`

## Plugin Protocol

- **Input**: IR JSON (same as `specforge emit` output) passed to `generate()`
- **Output**: `PluginResult` with `files` (path + content) and `errors`
- **File paths**: Relative to the output directory

## Example

See `examples/plugin-example/` for a minimal plugin that generates a README.

## IR Schema

See `assets/ir-schema.json` for the full IR JSON Schema.

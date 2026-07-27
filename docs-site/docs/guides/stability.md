---
title: Stability
sidebar_position: 2
description: Stability guarantees and versioning policy
---


# Stability Policy

## IR Schema

The IR (Intermediate Representation) emitted by `specforge emit` follows semver:

- **Patch** (0.2.x): No IR schema changes. Bug fixes and emitter improvements only.
- **Minor** (0.x.0): New optional fields may be added to the IR. Existing fields will not be removed or renamed. External emitters will continue to work.
- **Major** (x.0.0): Breaking IR schema changes allowed. External emitters may need updates.

### What counts as a breaking IR change:
- Removing or renaming a field
- Changing a field's type
- Changing the structure of a variant (e.g. adding a required field to Type::Scalar)
- Changing enum variant names (e.g. HttpMethod::Get → HttpMethod::GET)

### What does NOT count as a breaking IR change:
- Adding a new optional field with a default value
- Adding a new variant to an enum (external emitters should have a catch-all)
- Adding new operations or schemas to the output

## Generated SDKs

Generated SDKs are not versioned independently. The same spec + same specforge version always produces identical output (deterministic generation).

## CLI

- Subcommands may be added in minor versions
- Subcommands will not be removed without a major version bump
- Flags may be added in minor versions
- Flag removal requires a major version bump

## Deprecation Policy

Deprecated features emit a warning for at least one minor version before removal.

## OpenAPI Version Support

- **3.0.x**: Full support (primary target)
- **3.1.x**: Transparent downgrade to 3.0 for parsing. All 3.1 features that can be expressed in 3.0 are supported.

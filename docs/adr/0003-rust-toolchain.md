# ADR 0003: Rust Toolchain

Status: accepted

## Context

DevEnv is a Rust-based CLI tool. It should build as a single binary and remain portable across macOS, Linux, and Windows.

## Decision

Use Rust edition 2024 with minimum supported Rust version `1.85.0`.

The current development environment uses a newer Rust toolchain, but the workspace records `rust-version = "1.85.0"` so the intended minimum is explicit.

## Consequences

- The project can use Rust 2024 language features.
- CI should eventually test the minimum supported Rust version.
- Dependencies added later must support the chosen MSRV or require an ADR update.


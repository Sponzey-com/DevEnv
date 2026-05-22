# ADR 0005: Rustup Interop

## Status

Accepted

## Context

Rust is different from Java, Go, Node.js, and Python because `rustup` is the established toolchain manager. It already owns installation, updates, components, profiles, default toolchains, overrides, and cross-compilation targets.

DevEnv needs Rust support for the same local selection and shim workflows as other adapters, but replacing rustup would duplicate a mature ecosystem and expand scope too early.

## Decision

DevEnv will coexist with rustup instead of replacing it.

For the first Rust adapter:

- DevEnv manages runtime selection and activation only.
- DevEnv can register an existing Rust toolchain directory with `devenv add rust <path>`.
- DevEnv can discover explicit candidate paths and `RUSTUP_HOME/toolchains`.
- DevEnv does not run `rustup`, `rustc`, or `cargo` during default discovery or tests.
- DevEnv does not install Rust toolchains in this task.
- DevEnv does not manage components, profiles, channels, cross-compilation targets, or rustup overrides.
- DevEnv exposes `rustc` and `cargo` shims. It intentionally does not expose a `rustup` shim, because rustup is the manager, not the selected compiler toolchain.

Version detection is file-system based. A toolchain is valid when it has `bin/rustc`, `bin/cargo`, and either:

- a `VERSION` file with a value such as `rustc 1.85.0`, or
- a versioned rustup-style directory name such as `1.85.0-aarch64-apple-darwin`.

Channel-style rustup directories such as `stable-aarch64-apple-darwin` cannot be converted to an exact Rust version without executing `rustc` or querying rustup. DevEnv rejects that state with an actionable error instead of guessing.

## Consequences

- Rust support uses the same `ToolAdapter`, runtime registry, activation, and shim contracts as other languages.
- Default tests remain offline and do not depend on a real rustup installation.
- Users who want DevEnv to switch Rust versions can register versioned toolchain roots or point DevEnv at a rustup home that contains versioned toolchain directories.
- Users who rely on rustup channel names can continue using rustup directly until a future task adds a deliberate rustup query/delegation layer.

## Future Work

- Optional rustup-backed discovery that runs `rustc --version` or `rustup toolchain list` behind an explicit integration boundary.
- Optional `rust-toolchain.toml` and `rust-toolchain` compatibility.
- Component and target diagnostics, without making DevEnv a rustup replacement.
- DevEnv-owned Rust installs only if a later ADR shows clear value over rustup delegation.

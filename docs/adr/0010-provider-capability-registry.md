# ADR 0010: Provider Capability Registry

Status: accepted

## Context

DevEnv supports tools with different installation models. Go, Node.js, Terraform, OpenTofu, Flutter, and Java can be represented as Direct providers when metadata and checksums are available. Rust should coexist with rustup. Ruby and PHP are local-first until source-build or delegated install strategies are decided. Python has selection and fixture-backed install pipeline coverage, but live CPython Direct install is deferred by ADR 0008.

Without a central capability registry, CLI errors and documentation drift. Users can see an `install` command and assume every tool is equally installable, even when the correct action is `devenv add`, rustup, or an explicit provider selector.

## Decision

DevEnv will keep a built-in provider capability registry as the single source of truth for provider status.

Each provider capability records:

- canonical tool name;
- provider id;
- display name;
- support level: `Direct`, `Delegated`, or `LocalOnly`;
- source kind;
- checksum policy;
- selector dimensions such as Java distribution, Flutter channel, or Python implementation;
- supported platform matrix;
- direct-install unavailability reason when applicable;
- next action text for user-facing guidance.

CLI commands must derive provider status from this registry:

- `devenv provider list`;
- `devenv provider info <tool> [provider]`;
- `devenv metadata status [tool]`;
- unsupported `install` and `list-remote` errors;
- unsupported distribution/channel/provider errors.

Documentation should mirror the registry categories rather than introduce separate support labels.

## Support Levels

`Direct` means DevEnv owns metadata resolution, artifact download, checksum verification, extraction, install metadata, and activation for that provider path.

`Delegated` means another manager owns installation or update semantics. DevEnv may discover, register, select, and activate the runtime, but it should not pretend to replace the delegated manager.

`LocalOnly` means DevEnv only registers, discovers, selects, and activates runtimes already present on the machine. Remote install is intentionally not supported for that provider yet.

## Selector Policy

Provider selectors are explicit and capability-based.

Examples:

- Java supports `--distribution temurin`.
- Flutter supports `--channel stable`.
- Python records implementation as a provider dimension, but live CPython Direct install remains deferred.

Unsupported selector values should say which selector was rejected, list supported values, and point to `devenv provider info <tool>`.

## Consequences

- CLI output, errors, and docs can stay consistent as providers evolve.
- Non-Direct tools do not create false install expectations.
- Adding a new provider requires updating registry data, tests, and docs together.
- Future external plugin work can reuse the same capability shape before remote install is opened to plugins.

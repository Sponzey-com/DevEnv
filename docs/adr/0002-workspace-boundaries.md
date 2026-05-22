# ADR 0002: Workspace Boundaries

Status: accepted

## Context

DevEnv should follow Clean Architecture. The core must stay independent from CLI parsing, filesystems, network clients, shell rendering, archive extraction, and language-specific adapters.

## Decision

Start with a Rust workspace containing these crates:

- `devenv-core`: domain model, ports, and use cases.
- `devenv-adapters`: filesystem, network, shell, archive, and store implementations.
- `devenv-tools`: built-in tool adapters such as Java and Go.
- `devenv-cli`: command-line entry point and dependency composition root.

Dependency direction:

```text
devenv-cli -> devenv-core
devenv-cli -> devenv-adapters
devenv-cli -> devenv-tools
devenv-adapters -> devenv-core
devenv-tools -> devenv-core
```

`devenv-core` must not depend on outer crates.

## Consequences

- Core behavior can be tested without real IO.
- Java and Go support can be added as adapters instead of core special cases.
- The CLI can stay thin and avoid business rules.


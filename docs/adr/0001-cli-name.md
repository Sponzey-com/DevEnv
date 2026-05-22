# ADR 0001: CLI Binary Name

Status: accepted for initial development

## Context

DevEnv needs a command name for early CLI tests, documentation, and user workflow design.

The project name is DevEnv, but there is already a Nix-based open source project named `devenv`. This creates a possible naming collision for future package managers and public distribution channels.

## Decision

Chosen binary name: `devenv`.

Use `devenv` as the internal and early-development binary name while the product surface is still being validated.

Before public release, revisit whether the distributed binary should remain `devenv` or use a less ambiguous name.

## Consequences

- Early tests and examples can use the intended user-facing command.
- Release planning must include a naming review.
- Homebrew packaging must account for the possible conflict before publication.


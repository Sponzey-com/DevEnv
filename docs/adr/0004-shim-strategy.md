# ADR 0004: Shim Strategy

Status: accepted for initial development

## Context

DevEnv needs pyenv/jenv/goenv-style automatic switching when a user runs tools such as `java`, `javac`, `go`, or `gofmt` inside a project directory.

The shim layer must stay generic. Java and Go can expose different binary names, but dispatch must reuse the same config discovery, selection, activation, and command execution flow as `devenv exec`.

## Decision

Use script shims that forward into the single `devenv` dispatch binary:

```sh
exec devenv shim dispatch '<binary>' -- "$@"
```

`devenv shim rehash` generates one shim per exposed binary from adapter metadata. `devenv shim dispatch` resolves the owning tool from metadata, discovers the active selection from the current directory, builds the activation plan, and then executes the requested binary through the command runner path.

The target command is executed by name after prepending the selected runtime's `bin` directory to `PATH`. A `DEVENV_ACTIVE_SHIM` sentinel is set for the child process so recursive dispatch can be detected if the selected runtime does not actually provide the requested binary.

## Consequences

- The shim files are small and easy to inspect.
- `exec` and shim dispatch share selection and activation behavior.
- Adapter metadata controls which binaries receive shims.
- Shell profile files are never modified by shim generation or activation rendering.
- Public distribution may later replace script shims with native binary shims if startup overhead becomes material.

# ADR 0007: Plugin Strategy

Status: accepted for distribution planning

## Context

DevEnv now has enough built-in adapters to expose real extension pressure:

- SDK-style layouts: Java, Go, Flutter.
- single-binary tools: Terraform, OpenTofu.
- language runtimes with local-first behavior: Node.js, Python, Ruby, PHP, Rust.
- remote-install metadata and artifact resolution for several tools.
- activation and shim behavior shared across all adapters.

The project should prepare for external adapters, but the plugin ABI should not be fixed before release packaging and operational expectations are clear.

## Options Considered

### Manifest-only plugins

A manifest-only model can describe tool names, aliases, exposed binaries, config files, and static archive URLs. It is simple to validate and safe to inspect.

It is not enough for tools that need custom version normalization, platform mapping, release metadata parsing, install validation, or activation logic. Most existing adapters already need behavior beyond static metadata.

### Executable JSON protocol plugins

An external executable can communicate through stdin/stdout JSON messages. DevEnv controls when it is invoked, validates every response, and still owns installation, checksum verification, store writes, activation rendering, and shim generation.

This model works across implementation languages and avoids loading untrusted code into the DevEnv process. It also keeps plugin failures isolated from the core process.

### WebAssembly plugins

Wasm gives a stronger sandbox story and can be deterministic when the host interface is narrow. It also adds toolchain, runtime, WASI, debugging, and distribution complexity before DevEnv has a public plugin ecosystem.

### Rust dynamic library plugins

Rust dynamic libraries would expose the most direct adapter API, but they create ABI stability, compiler version, platform loading, crash isolation, and unsafe-code trust problems. They also force plugin authors toward Rust.

## Decision

Do not ship a public plugin ABI in the first distribution release.

Use this staged strategy:

1. Keep built-in adapters as the reference implementation for the core adapter contract.
2. Allow manifest-only metadata to inform future plugin descriptors, but do not treat manifests as the primary plugin model.
3. Make an executable JSON protocol the preferred first public plugin MVP.
4. Defer Wasm until sandboxing requirements justify the added runtime complexity.
5. Reject Rust dynamic library plugins for the public extension model unless a future ADR identifies a narrow, internal-only use case.

The executable JSON protocol should start with read-only and deterministic operations:

- describe tool metadata and exposed binaries
- list local candidate runtimes
- normalize versions
- validate registered runtime paths
- produce activation operations

Remote install support for external plugins should come later. DevEnv must keep ownership of downloads, checksums, extraction, install transactions, and store metadata.

## Security And Validation Notes

When a plugin MVP is implemented:

- Plugin manifests must declare the executable path, supported protocol version, tool names, and required capabilities.
- DevEnv must reject duplicate tool names or duplicate shim binaries before invoking plugins.
- Every plugin response must be schema-validated and size-limited.
- Plugin execution should use timeouts and actionable error messages.
- Plugins must not mutate shell profiles or DevEnv store files directly.
- Install-related capabilities must be explicit and disabled by default.
- Network access should remain a DevEnv-owned concern until a separate policy exists.

## Consequences

- Early releases keep extension risk low while still preserving a clear path to plugins.
- The current Clean Architecture boundary remains useful: plugin commands will implement outer adapter behavior, not core policy.
- Built-in adapters continue to prove which operations belong in the protocol.
- Distribution can move forward without committing to an unstable ABI.

# ADR 0009: Metadata Provider And Cache

Status: accepted

## Context

Phase 002 moves DevEnv beyond a remote metadata path that depended only on environment fixtures. `list-remote` and `install` need a product path where DevEnv can refresh small provider metadata, cache it, and later use it to resolve one platform artifact. This must not turn every normal test or local command into a network operation.

Runtime metadata and runtime artifacts have different operational costs:

- metadata payloads are small, parseable, and safe to refresh often;
- runtime artifacts are large archives or binaries and should only download during `install`;
- offline development, CI, mirrors, and air-gapped environments still need deterministic fixture inputs.

## Decision

Keep metadata refresh separate from artifact download.

DevEnv will use this source priority for remote metadata:

1. Explicit fixture or source override, such as `DEVENV_GO_RELEASE_METADATA`.
2. DevEnv-owned metadata cache.
3. Official provider refresh when the command explicitly asks for refresh or metadata update.
4. Stale cache fallback where the provider path supports it.

ADR 0011 extends this priority for Phase 003 by adding an explicit DevEnv GitHub metadata catalog and mirror catalog sources between local cache and upstream official provider refresh. ADR 0012 defines the trust policy for that catalog.

`--offline` disables official network refresh. In offline mode, a command may read fixture overrides or existing cache entries. If neither exists, it must fail with a message that explains how to seed the cache.

Metadata cache entries live under:

```text
$DEVENV_HOME/cache/metadata/<tool>/<provider>/metadata.json
```

The cache file is a DevEnv-owned envelope, not user configuration. It records the schema version, tool, provider, selector, source URL, fetch time, TTL, validator metadata, payload hash, payload kind, and payload. Users should seed metadata through fixture variables or provider settings rather than editing cache files by hand.

The default TTL is 24 hours for Phase 002 direct providers. Provider-specific TTL changes require a provider decision, not an ad hoc CLI branch.

## Provider Payload Policy

Provider parsers own upstream payload details:

- Go official JSON is parsed by the Go provider code.
- Node official index and SHASUMS files are parsed by the Node provider code.
- Flutter release JSONs are parsed by the Flutter provider code.
- Terraform/OpenTofu release and checksum files are parsed by their provider code.
- Java Temurin API payloads are parsed by the Java provider code.

Core and CLI code should work with normalized release metadata and cache envelopes. They should not depend on upstream field names.

Fixture variables remain first-class:

- normalized fixture variables support fast offline tests and internal mirrors;
- official-payload fixture variables support parser tests and air-gapped cache seeding;
- fake HTTP base URL variables support mirror smoke tests without changing provider code.

## Artifact Download Policy

Artifact download happens only during `devenv install`, not during metadata refresh.

Downloaded artifacts use a DevEnv-owned download cache under:

```text
$DEVENV_HOME/cache/downloads
```

Checksum-bearing artifacts are promoted into a checksum-addressed cache path only after verification. A partial or checksum-failing download must not be promoted. Providers without usable checksums are excluded from default Direct install unless a future ADR defines an explicit opt-in policy.

## Mirror And Air-Gapped Use

Mirrors should expose the same logical provider payloads as official providers. A mirror may be:

- a normalized fixture file;
- a directory of official response fixtures;
- a fake or internal HTTP base URL with the same file layout as the official provider path.

Provider source configuration belongs in user/global configuration or environment, not project version files. Project files should say which runtime version to use. Provider configuration should say where DevEnv obtains metadata and artifacts.

## Consequences

- Default tests remain offline.
- `metadata status` can describe fresh, stale, missing, and corrupt cache states.
- `metadata update` refreshes metadata only.
- `install` may reuse cached metadata and downloads only the selected artifact.
- Mirrors and air-gapped environments can seed cache or override sources without changing project version selections.

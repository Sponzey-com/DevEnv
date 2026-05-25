# ADR 0011: GitHub Metadata Catalog

Status: accepted for Phase 003 implementation

## Context

Phase 002 added provider capabilities, metadata cache, official metadata refresh paths, and download cache behavior. The remaining product problem is operational: DevEnv should not need a new binary release every time an upstream runtime publishes a new version, and users should not have to absorb every upstream API shape change directly.

DevEnv needs a managed metadata source between upstream official providers and the CLI. This source must preserve the existing offline-first development model and must not become a runtime artifact hosting service.

## Decision

Introduce a DevEnv-managed GitHub metadata catalog.

The catalog is a normalized metadata mirror, not a replacement for official providers. It stores small metadata payloads that describe upstream runtime artifacts. It does not store runtime artifact archives.

The catalog will be distributed from an immutable GitHub Release asset or immutable tag-based URL. A mutable `main` branch raw URL may be used for development and verification, but it is not the default trust source.

Catalog metadata is consumed only after trust and integrity verification, as defined in ADR 0012.

## Non-Goals

- Do not upload Java, Go, Node.js, Flutter, Terraform, OpenTofu, or other runtime artifact archives into the metadata catalog repository.
- Do not remove official provider refresh paths.
- Do not remove environment fixture overrides.
- Do not introduce a lockfile in Phase 003.
- Do not make the catalog a package manager or project dependency resolver.
- Do not make catalog network checks part of the default offline test suite.

## Source Priority

Phase 003 uses this metadata source priority:

1. Command or environment explicit fixture override.
2. Explicit catalog file or base URL override.
3. User/global mirror config or mirror environment.
4. Fresh local metadata cache.
5. DevEnv GitHub metadata catalog.
6. Upstream official provider refresh.
7. Stale cache fallback when policy allows it.

`--offline` disables the network sources: default GitHub catalog and upstream official provider refresh. Offline commands may still use fixture overrides, explicit file catalog sources, mirror file URLs, and local cache.

When the user explicitly selects `--source catalog`, catalog errors are not hidden. When the user explicitly selects `--source official`, catalog is skipped.

## Repository Shape

The recommended repository is separate from the DevEnv source repository:

```text
github.com/<org>/devenv-metadata
  README.md
  catalog-version
  schema/
    v1/catalog.schema.json
    v1/tool-metadata.schema.json
  v1/
    manifest.json
    manifest.sig
    SHA256SUMS
    tools/
      go/official/releases.json
      node/official/releases.json
      java/temurin/releases.json
      flutter/stable/releases.json
      terraform/hashicorp/releases.json
      opentofu/opentofu/releases.json
  scripts/
    update-go
    update-node
    verify-catalog
```

Release asset candidates:

- `devenv-catalog-v1-<catalog_version>.tar.gz`
- `manifest.json`
- `manifest.sig`
- `SHA256SUMS`

The catalog repository may store upstream raw source payloads for review under a separate `sources/` directory, but the CLI consumes normalized catalog payloads.

## Manifest Requirements

The manifest must include:

- schema version;
- catalog id;
- generated timestamp;
- expiration timestamp;
- catalog version;
- minimum supported DevEnv version;
- monotonic sequence;
- entries for each tool/provider payload;
- sha256 digest for each payload;
- payload kind and TTL.

The manifest is signed. Payload files are validated against the manifest digests.

## Rollout Policy

Catalog source is enabled in stages.

### Experimental

Catalog is only used when the user opts in with `--source catalog`, an explicit catalog URL/file override, or `DEVENV_ENABLE_CATALOG=1`.

Required before leaving Experimental:

- Go catalog metadata update and offline reuse work.
- Node catalog metadata update and offline reuse work.
- trust failure messages are tested.
- opt-in catalog smoke is available.

### Beta Default

Catalog is included in `--source auto` for selected providers after Go and Node catalog smoke checks are stable.

Required before leaving Beta:

- rollback procedure is documented.
- mirror and air-gapped documentation exists.
- release notes explain the source priority change.
- official provider fallback or manual override remains available.

### Stable Default

Catalog becomes the preferred default metadata source after repository publishing, signing, key rotation, mirror docs, and rollback procedures have been exercised.

## Failure Policy

Network failures and trust failures are different classes.

| Failure | Example | Fallback |
| --- | --- | --- |
| Network failure | timeout, DNS failure, GitHub 5xx, 404 for catalog asset | In `auto`, may fall back to official provider or stale cache. In `catalog`, fail. |
| Trust failure | signature mismatch, unknown trust root, payload checksum mismatch, manifest checksum mismatch | Do not silently fall back. Fail with explicit trust guidance. |
| Compatibility failure | `min_devenv_version` is higher than the running CLI | Fail with upgrade guidance. |
| Expired catalog | `expires_at` is in the past | Treat as stale. Use fallback only when policy allows it. |

Trust failure must not write cache entries.

## Mirror And Air-Gapped Use

Organizations may mirror the catalog by copying a verified release asset or checked-out tag into an internal file or HTTP location.

Example:

```text
online machine
  -> download devenv-catalog-v1-<catalog_version>.tar.gz
  -> verify manifest and signature
  -> copy archive or extracted v1 directory to internal mirror

air-gapped machine
  -> DEVENV_CATALOG_BASE_URL=file:///mirror/devenv-metadata/v1
  -> devenv metadata update go --source catalog
  -> devenv list-remote go --offline
```

Mirror configuration belongs in environment or future user/global config, not project config. Project config selects versions. Provider source config decides where metadata is obtained.

## Consequences

- DevEnv can publish metadata updates without releasing a new CLI binary.
- Users gain a stable normalized metadata path while retaining official provider and fixture fallback paths.
- The catalog repository becomes part of the DevEnv supply-chain surface.
- Phase 003 must implement signature and checksum enforcement before catalog becomes a default source.
- Catalog version and payload digests should be recorded in DevEnv-owned install metadata to support a future lockfile.

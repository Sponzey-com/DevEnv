# Catalog Generator

작성일: 2026-05-22

`devenv-catalog`는 upstream official metadata를 DevEnv catalog v1 payload와 manifest로 변환하는 초안 도구다. 운영용 GitHub Actions와 production signing backend는 아직 범위 밖이며, 현재 목적은 deterministic output, review 가능한 diff, local verification을 고정하는 것이다.

## 위치

- Rust implementation: `crates/devenv-catalog`
- Generate wrapper: `scripts/catalog-generate`
- Verify wrapper: `scripts/catalog-verify`
- Source fixture: `fixtures/catalog-generator/source`
- Override fixture: `fixtures/catalog-generator/overrides.toml`
- Generated sample: `fixtures/catalog-generated/v1`

## Source Layout

```text
source/
  go/official/releases.json
  node/official/index.json
  node/official/shasums/v<version>/SHASUMS256.txt
```

The source directory keeps raw upstream-shaped payloads separate from generated catalog output. The generated catalog writes normalized payloads under `tools/<tool>/<provider>/releases.json` and a root `manifest.json`.

## Generate

```sh
scripts/catalog-generate \
  --source fixtures/catalog-generator/source \
  --output fixtures/catalog-generated/v1 \
  --generated-at 2026-05-22T00:00:00Z \
  --expires-at 2026-05-29T00:00:00Z \
  --catalog-version 2026.05.22.1 \
  --overrides fixtures/catalog-generator/overrides.toml
```

Generated output is deterministic when the inputs and timestamps are fixed:

- manifest entries are sorted by tool and provider.
- releases are sorted by normalized version descending.
- artifacts are sorted by filename.
- JSON is pretty-printed with a trailing newline.
- manifest `sha256` entries are computed from the exact payload bytes.

## Verify

```sh
scripts/catalog-verify --catalog fixtures/catalog-generated/v1
```

The verifier checks:

- `manifest.sig` matches `sha256:<manifest bytes>`.
- every manifest entry payload exists.
- every manifest entry `sha256` matches payload bytes.
- payload `schema_version`, `tool`, and `provider` match the manifest entry.

## Overrides

Manual policy inputs live in a small TOML file:

```toml
[[release]]
tool = "go"
version = "1.21.0"
stable = false
yanked = true
deprecated = true
reason = "manual yanked release for generator fixture"
```

Overrides are intentionally small and reviewable. They are used for yanked/deprecated/stability metadata that may not exist in upstream payloads or that DevEnv wants to adjust before publishing a catalog.

## Current Scope

Implemented provider generators:

- Go official JSON.
- Node official index plus SHASUMS256 files.

Not implemented in this task:

- GitHub Actions.
- publishing to a real metadata repository.
- production signing backend.
- every DevEnv provider generator.

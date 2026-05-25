# DevEnv Metadata Repository Workflow

작성일: 2026-05-23

## 목적

이 문서는 별도 `devenv-metadata` repository를 bootstrap하기 위한 운영 초안이다. 실제 GitHub repository, GitHub Actions secret, production signing backend를 지금 생성하지 않는다. 목표는 catalog metadata를 refresh, verify, sign, publish, rollback하는 절차를 문서로 고정해 DevEnv CLI가 immutable catalog artifact를 신뢰할 수 있게 만드는 것이다.

관련 결정:

- ADR 0011: GitHub metadata catalog는 runtime artifact archive를 저장하지 않는 normalized metadata mirror다.
- ADR 0012: catalog manifest는 pinned trust root로 signature를 검증하고, 각 payload는 manifest entry의 sha256으로 검증한다.
- Catalog schema v1: `docs/catalog/schema-v1.md`
- Catalog generator: `docs/catalog/generator.md`

## 원칙

- `main` 또는 다른 mutable branch의 raw branch URL은 기본 distribution source가 아니다.
- mutable raw branch URL은 development, PR preview, manual verification 용도로만 사용한다.
- 기본 배포 단위는 immutable GitHub Release asset 또는 immutable tag 기반 URL이다.
- Release asset은 한 번 publish된 뒤 다시 쓰지 않는다. 잘못된 release는 새 catalog version으로 교정한다.
- catalog repository에는 Go, Java, Node.js, Flutter, Terraform, OpenTofu 같은 runtime archive를 저장하지 않는다.
- catalog에는 artifact URL, checksum, platform, provider selector, yanked/deprecated policy 같은 작은 metadata만 저장한다.
- verifier가 실패하면 publish workflow는 release asset을 만들지 않는다.
- trust failure는 official provider fallback으로 숨기지 않는다.

## Repository Layout

권장 repository:

```text
github.com/<org>/devenv-metadata
  README.md
  catalog-version
  schema/
    v1/catalog.schema.json
    v1/tool-metadata.schema.json
  sources/
    go/official/releases.json
    node/official/index.json
    node/official/shasums/v20.11.1/SHASUMS256.txt
  overrides/
    policy.toml
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
    catalog-generate
    catalog-verify
  .github/
    workflows/
      refresh.yml
      verify.yml
      publish.yml
```

Directory responsibilities:

| Path | Responsibility |
| --- | --- |
| `catalog-version` | 현재 candidate catalog version. 예: `2026.05.23.1`. |
| `schema/` | JSON schema 또는 schema 문서 snapshot. CLI parser와 generator 변경 시 함께 갱신한다. |
| `sources/` | upstream official raw payload. Review와 regeneration을 돕기 위한 입력이며 CLI distribution payload가 아니다. |
| `overrides/` | yanked, deprecated, stable flag, reason 같은 manual policy input. 작은 TOML 파일로 유지한다. |
| `v1/` | generated normalized catalog output. CLI가 소비하는 publish candidate다. |
| `scripts/` | DevEnv source repository의 `scripts/catalog-generate`, `scripts/catalog-verify`와 동등한 wrapper. |

`sources/`를 repository에 보관할지는 provider별로 선택할 수 있다. 보관하더라도 release asset에는 normalized `v1/` catalog와 manifest/signature만 포함한다.

## Catalog Versioning

Catalog version은 사람이 읽을 수 있고 정렬 가능한 값을 사용한다.

```text
YYYY.MM.DD.N
```

예:

```text
2026.05.23.1
2026.05.23.2
```

규칙:

- 같은 날짜에 여러 번 publish하면 마지막 숫자를 증가시킨다.
- `manifest.sequence`는 같은 `catalog_id`와 trust root에서 단조 증가한다.
- 이미 publish된 `catalog_version`과 `sequence`는 재사용하지 않는다.
- rollback도 과거 asset rewrite가 아니라 더 높은 `catalog_version`과 `sequence`를 가진 새 release로 처리한다.

## Refresh Workflow

Refresh workflow는 upstream metadata를 가져오고 generated catalog diff를 만드는 PR용 workflow다. 이 workflow는 publish하지 않는다.

Trigger:

- scheduled run, 예: 하루 1회.
- manual `workflow_dispatch`.
- provider별 manual input, 예: `tool=go,node`.

Expected output:

- `sources/` raw payload update.
- `v1/tools/<tool>/<provider>/releases.json` update.
- `v1/manifest.json` update.
- `v1/manifest.sig`는 publish workflow에서 최종 signing한다. PR refresh 단계에서 생성되는 local verifier용 placeholder가 있다면 distribution signature로 취급하지 않는다.
- verifier output summary.
- generated diff.

Draft:

```yaml
name: refresh catalog

on:
  schedule:
    - cron: "17 2 * * *"
  workflow_dispatch:
    inputs:
      tool:
        description: "Optional tool filter such as go or node"
        required: false
        type: string

permissions:
  contents: write
  pull-requests: write

jobs:
  refresh:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v6
      - uses: actions-rust-lang/setup-rust-toolchain@v1
        with:
          toolchain: stable
      - name: Fetch upstream raw metadata
        run: |
          scripts/fetch-upstream-metadata "${{ inputs.tool }}"
      - name: Generate catalog
        run: |
          scripts/catalog-generate \
            --source sources \
            --output v1 \
            --generated-at "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
            --expires-at "$(date -u -d '+7 days' +%Y-%m-%dT%H:%M:%SZ)" \
            --catalog-version "$(cat catalog-version)" \
            --overrides overrides/policy.toml
      - name: Verify generated catalog
        run: |
          scripts/catalog-verify --catalog v1
      - name: Open refresh PR
        uses: peter-evans/create-pull-request@v6
        with:
          branch: catalog-refresh/${{ github.run_id }}
          title: "Refresh DevEnv catalog metadata"
          commit-message: "Refresh generated catalog metadata"
          body: |
            Generated by refresh workflow.

            Review requirements:
            - Inspect generated diff under v1/.
            - Inspect yanked/deprecated override changes.
            - Confirm verifier output passed.
            - Confirm no runtime archive was added.
```

Notes:

- The exact upstream fetch implementation is provider-specific and can call `devenv-catalog` helpers once those exist.
- macOS `date -v+7d` and GNU `date -d '+7 days'` differ. GitHub hosted Ubuntu runners use GNU date.
- Refresh PRs must not publish release assets.

## Verify Workflow

Verify workflow runs on every PR and on `main`. It ensures generated output is internally consistent and safe to review.

Required checks:

- schema validation for `manifest.json` and payloads.
- `scripts/catalog-verify --catalog v1`.
- generated output is deterministic: run generator into a temp directory with fixed timestamps and compare.
- no runtime archive files are present.
- `manifest.entries[].sha256` matches payload bytes.
- unsafe relative paths are rejected by verifier tests.
- signing policy files are unchanged unless explicitly reviewed.

Draft:

```yaml
name: verify catalog

on:
  pull_request:
  push:
    branches: [main]

permissions:
  contents: read

jobs:
  verify:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v6
      - uses: actions-rust-lang/setup-rust-toolchain@v1
        with:
          toolchain: stable
      - name: Verify catalog
        run: |
          scripts/catalog-verify --catalog v1
      - name: Reject runtime archives
        run: |
          find . \
            \( -name '*.tar.gz' -o -name '*.tar.xz' -o -name '*.zip' -o -name '*.pkg' -o -name '*.msi' -o -name '*.dmg' \) \
            -not -path './.git/*' \
            -print \
            -quit | tee /tmp/archive-match
          test ! -s /tmp/archive-match
      - name: Check generated diff policy
        run: |
          git diff --check
```

Branch protection should require this `verify catalog` workflow before merge.

## Publish Workflow

Publish workflow creates immutable Release assets from a verified catalog. It should only run from `main` and only for a catalog version that has already passed PR review.

Trigger:

- manual `workflow_dispatch` with `catalog_version`.
- optional tag push after the manual workflow is stable.

Release asset candidates:

- `devenv-catalog-v1-<catalog_version>.tar.gz`
- `manifest.json`
- `manifest.sig`
- `manifest.cert` 또는 `manifest.bundle`, keyless verifier가 요구하는 경우
- `SHA256SUMS`

Draft:

```yaml
name: publish catalog

on:
  workflow_dispatch:
    inputs:
      catalog_version:
        description: "Catalog version to publish, for example 2026.05.23.1"
        required: true
        type: string

permissions:
  contents: write
  id-token: write

jobs:
  publish:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v6
      - uses: actions-rust-lang/setup-rust-toolchain@v1
        with:
          toolchain: stable
      - name: Confirm version
        run: |
          test "$(cat catalog-version)" = "${{ inputs.catalog_version }}"
      - name: Verify before signing
        run: |
          scripts/catalog-verify --catalog v1
      - name: Sign manifest
        run: |
          cosign sign-blob \
            --yes \
            --output-signature v1/manifest.sig \
            --output-certificate v1/manifest.cert \
            v1/manifest.json
      - name: Verify signature
        run: |
          cosign verify-blob \
            --signature v1/manifest.sig \
            --certificate v1/manifest.cert \
            --certificate-identity-regexp '^https://github.com/<org>/devenv-metadata/.github/workflows/publish.yml@refs/heads/main$' \
            --certificate-oidc-issuer https://token.actions.githubusercontent.com \
            v1/manifest.json
      - name: Create release archive
        run: |
          tar -czf "devenv-catalog-v1-${{ inputs.catalog_version }}.tar.gz" v1
          shasum -a 256 "devenv-catalog-v1-${{ inputs.catalog_version }}.tar.gz" > SHA256SUMS
          shasum -a 256 v1/manifest.json >> SHA256SUMS
          shasum -a 256 v1/manifest.sig >> SHA256SUMS
          shasum -a 256 v1/manifest.cert >> SHA256SUMS
      - name: Create GitHub Release
        uses: softprops/action-gh-release@v2
        with:
          tag_name: catalog-${{ inputs.catalog_version }}
          name: DevEnv catalog ${{ inputs.catalog_version }}
          files: |
            devenv-catalog-v1-${{ inputs.catalog_version }}.tar.gz
            v1/manifest.json
            v1/manifest.sig
            v1/manifest.cert
            SHA256SUMS
          fail_on_unmatched_files: true
```

Publish gates:

- `scripts/catalog-verify --catalog v1` must pass before signing.
- signing verification must pass before creating the GitHub Release.
- the tag name must be unique.
- the release body should include verifier output, catalog version, manifest sha256, sequence, and changed providers.
- the workflow must not upload runtime archives.

## Signing And Secret Handling

ADR 0012 requires a pinned trust root and does not allow plain adjacent checksums to replace signature verification.

Recommended Phase 003 signing strategy:

- use cosign keyless signing for the official `devenv-metadata` publish workflow;
- pin the expected GitHub workflow identity and OIDC issuer in DevEnv trust policy;
- keep `id-token: write` limited to publish workflow;
- do not store long-lived signing private keys in repository secrets for the default official catalog;
- if an organization mirror needs its own trust root, require explicit user/global trust configuration.

Candidate trust identity:

```text
issuer: https://token.actions.githubusercontent.com
identity: https://github.com/<org>/devenv-metadata/.github/workflows/publish.yml@refs/heads/main
```

If keyless signing is not acceptable for an organization mirror, use a separate organization-specific signing key and keep it outside project configuration. Project repositories must not be able to change catalog trust roots silently.

Secret handling rules:

- refresh and verify workflows do not receive signing authority.
- publish workflow receives only the minimum signing permission.
- release tokens and signing credentials must not be available to pull requests from forks.
- signing policy changes require maintainer approval, ADR update when public trust root changes, and release notes.

## PR Review Policy

Every generated catalog PR must include:

- changed provider list;
- catalog version and sequence change;
- `scripts/catalog-verify --catalog v1` result;
- generated diff summary;
- yanked/deprecated override diff, if any;
- confirmation that no runtime archive was added;
- links to upstream release notes or source payloads for newly added major/minor lines when available.

Review checklist:

- `v1/manifest.json` changed only as expected.
- `entries[].path` points under `tools/<tool>/<provider>/`.
- `entries[].sha256` changes match payload changes.
- new artifact URLs use the expected upstream provider domains.
- installable artifacts have `checksum_algorithm="sha256"` and `checksum="sha256:<hex>"`.
- checksum-missing artifacts are either absent or `installable=false` with `install_block_reason`.
- yanked releases have `reason`.
- deprecated releases have `reason` when policy-driven.
- generated ordering is stable and not churned by timestamps outside expected fields.
- no runtime archive, package installer, or executable binary was committed.

Generated diff policy:

- generated payload files may be large, but ordering must be deterministic.
- override files should stay small and human-written.
- raw upstream payload updates must be separated from manual override policy changes when practical.
- if a generator change rewrites many payloads, include the generator version change and a before/after verifier summary.

## Rollback Procedure

Rollback does not mutate or delete a trusted Release asset.

Bad metadata with valid trust:

1. Open an incident issue with affected `catalog_version`, `sequence`, tools, providers, and affected runtime versions.
2. Add a manual override to yank or deprecate the bad release, or correct the generated source input.
3. Generate a new catalog with a higher `catalog_version` and higher `sequence`.
4. Run verify workflow.
5. Publish a new immutable Release asset.
6. Update release notes and incident issue with the fixed catalog version.

Compromised signing identity or trust root:

1. Stop publish workflow by disabling branch protection bypass and release permissions.
2. Ship a DevEnv binary update that revokes the affected trust root or identity.
3. Publish a new ADR or ADR update for key rotation.
4. Reissue catalog metadata under the new trust root after verification.

Expired catalog:

1. Run refresh workflow.
2. Publish a new catalog version.
3. Keep old immutable releases available for investigation.

Emergency user guidance:

- users can select `--source official` when the issue is catalog metadata freshness or ordinary catalog network failure;
- users should not be told to ignore catalog trust failures;
- organizations can pin an older known-good immutable release only through explicit user/global trust policy.

## Branch Protection

Recommended branch protection for `main`:

- require pull request review from at least one maintainer;
- require `verify catalog` workflow;
- require signed commits or ruleset-based provenance if the organization uses it;
- require linear history or squash merge to keep catalog changes auditable;
- restrict who can push tags matching `catalog-*`;
- restrict who can run publish workflow;
- prevent force pushes and branch deletion.

Recommended release protection:

- only publish from `main`;
- use tag names `catalog-<catalog_version>`;
- do not allow release asset overwrite;
- keep Release assets public and immutable once trusted.

## Bootstrap Sequence

1. Create `github.com/<org>/devenv-metadata`.
2. Copy schema, generator wrapper, verifier wrapper, and initial `v1/` generated catalog.
3. Add `refresh.yml`, `verify.yml`, and `publish.yml` from this document.
4. Configure branch protection and required checks.
5. Run refresh workflow and review the generated PR.
6. Run publish workflow manually for the first catalog version.
7. Record Release asset URL, manifest sha256, signing identity, and catalog version in DevEnv release notes.
8. Run `DEVENV_CATALOG_SMOKE=1 DEVENV_CATALOG_SMOKE_BASE_URL=<published-v1-root> scripts/catalog-smoke.sh` from the DevEnv source repository.
9. Keep DevEnv CLI catalog use experimental until signature verification and opt-in smoke tests are stable.

## Consumer Distribution Contract

DevEnv clients should consume one of these immutable forms:

```text
https://github.com/<org>/devenv-metadata/releases/download/catalog-2026.05.23.1/devenv-catalog-v1-2026.05.23.1.tar.gz
https://github.com/<org>/devenv-metadata/releases/download/catalog-2026.05.23.1/manifest.json
https://github.com/<org>/devenv-metadata/releases/download/catalog-2026.05.23.1/manifest.sig
https://github.com/<org>/devenv-metadata/releases/download/catalog-2026.05.23.1/manifest.cert
```

The default DevEnv distribution source must not be:

```text
https://raw.githubusercontent.com/<org>/devenv-metadata/main/v1/manifest.json
```

Raw branch URLs are acceptable only for explicit development overrides and must be labeled as mutable in diagnostics.

# DevEnv Metadata Catalog Bootstrap

작성일: 2026-05-23

## 목적

이 문서는 별도 `devenv-metadata` repository를 처음 만들 때 필요한 최소 절차를 정리한다. 자세한 운영 workflow는 `docs/catalog/repository-workflow.md`가 기준이다. 이 문서는 실제 GitHub repository를 생성하거나 signing secret을 등록하지 않는다.

## 전제

- Catalog는 runtime artifact archive를 저장하지 않는다.
- Catalog는 작은 normalized metadata만 저장한다: artifact URL, checksum, platform, archive type, provider selector, release channel, yanked/deprecated policy.
- DevEnv CLI는 catalog manifest signature를 pinned trust root로 검증하고, payload는 manifest entry checksum으로 검증한다.
- Mutable raw branch URL은 development 용도다. 기본 distribution source는 immutable GitHub Release asset 또는 immutable tag 기반 URL이어야 한다.
- Fixture overrides remain supported and keep higher priority than catalog source selection.

## 초기 Repository 구조

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
    tools/
      go/official/releases.json
      node/official/releases.json
  scripts/
    catalog-generate
    catalog-verify
  .github/
    workflows/
      refresh.yml
      verify.yml
      publish.yml
```

`sources/` is optional but useful for PR review. `v1/` is the normalized output consumed by DevEnv. Release archives should package `v1/` plus manifest signature material, not upstream runtime archives.

## Bootstrap Steps

1. Create the repository.

   ```sh
   gh repo create <org>/devenv-metadata --private
   git clone git@github.com:<org>/devenv-metadata.git
   cd devenv-metadata
   ```

2. Copy schema and initial catalog files from the DevEnv source repository.

   ```sh
   mkdir -p schema/v1 scripts overrides sources v1
   cp ../Sponzey\ DevEnv/docs/catalog/schema-v1.md schema/v1/README.md
   cp ../Sponzey\ DevEnv/scripts/catalog-generate scripts/catalog-generate
   cp ../Sponzey\ DevEnv/scripts/catalog-verify scripts/catalog-verify
   cp -R ../Sponzey\ DevEnv/fixtures/catalog-generated/v1/. v1/
   cp ../Sponzey\ DevEnv/fixtures/catalog-generator/overrides.toml overrides/policy.toml
   ```

3. Set the initial catalog version.

   ```sh
   printf "2026.05.23.1\n" > catalog-version
   ```

4. Verify the initial catalog locally.

   ```sh
   scripts/catalog-verify --catalog v1
   devenv metadata verify-catalog go --catalog v1 --source file
   ```

5. Add workflow files.

   Use the `refresh.yml`, `verify.yml`, and `publish.yml` drafts from `docs/catalog/repository-workflow.md`. The verify workflow should be required before merge. The publish workflow should be manually triggered until catalog publishing is proven stable.

6. Configure branch protection.

   Required policy:

   - require pull request review;
   - require the catalog verify workflow;
   - restrict `catalog-*` tag creation;
   - restrict publish workflow execution;
   - prevent force pushes.

7. Publish the first catalog release only after verification passes.

   Candidate Release assets:

   ```text
   devenv-catalog-v1-2026.05.23.1.tar.gz
   manifest.json
   manifest.sig
   manifest.cert
   SHA256SUMS
   ```

## Signing And Trust Root

The official catalog should use the accepted Phase 003 trust policy:

- sign `v1/manifest.json`;
- verify the signature before creating Release assets;
- pin the expected GitHub workflow identity or signing public key in DevEnv's trust root policy;
- keep signing authority out of refresh and verify workflows;
- do not allow project config to change catalog trust roots.

Current local fixtures may use a digest-backed `manifest.sig` for deterministic development. That is not a production signing backend. Production catalog publishing must use the trust root policy documented in ADR 0012 and `docs/catalog/repository-workflow.md`.

## Mirror And Air-Gapped Bootstrap

Connected staging machine:

```sh
curl -fsSLO https://github.com/<org>/devenv-metadata/releases/download/catalog-2026.05.23.1/devenv-catalog-v1-2026.05.23.1.tar.gz
tar -xzf devenv-catalog-v1-2026.05.23.1.tar.gz
devenv metadata verify-catalog go --catalog v1 --source file
rsync -a v1/ /mirror/devenv-metadata/v1/
```

Air-gapped machine:

```sh
export DEVENV_ENABLE_CATALOG=1
export DEVENV_CATALOG_BASE_URL=file:///mirror/devenv-metadata/v1

devenv metadata verify-catalog go --catalog /mirror/devenv-metadata/v1 --source file
devenv metadata update go --source catalog
devenv list-remote go --offline
```

Project config should still contain only selected versions. Catalog and mirror source settings belong to environment variables or future user/global provider configuration.

## Troubleshooting

`catalog unavailable`:

- The catalog base URL or local path is missing.
- Set `DEVENV_CATALOG_BASE_URL`, pass `--catalog <path-or-url>`, or use `--source official` when official provider fallback is acceptable.

`catalog network failure`:

- HTTP fetch failed due to DNS, timeout, 404, or server error.
- In `auto`, policy may allow official provider or stale cache fallback. In `--source catalog`, the command fails.

`catalog trust failure`:

- Signature, trust root, catalog freshness, schema version, or payload checksum verification failed.
- Do not ignore it. Use a newer signed catalog release, repair the mirror, or inspect trust root configuration.

`metadata_source` in `devenv metadata status <tool>`:

- `catalog` means the cache was populated from a verified catalog payload.
- `official` means provider refresh populated the cache.
- `env` means a fixture override populated the cache.
- `stale-cache` means the cache is expired but still visible for fallback policy.

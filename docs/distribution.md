# DevEnv Distribution

DevEnv distribution starts with GitHub release artifacts and an organization Homebrew tap. Direct `brew install devenv` through `homebrew/core` is a later goal.

## Release Artifacts

Create a tag:

```sh
git tag v<version>
git push origin v<version>
```

The release workflow builds:

- `aarch64-apple-darwin`
- `x86_64-apple-darwin`
- `x86_64-unknown-linux-gnu`
- `aarch64-unknown-linux-gnu` through `cross`
- `x86_64-pc-windows-msvc`

Artifacts are named:

```text
devenv-<version>-<target>.tar.gz
devenv-<version>-<target>.tar.gz.sha256
SHA256SUMS
```

Each package contains:

- `devenv` or `devenv.exe`
- `USER_GUIDE.md`

The release workflow runs a packaged-binary smoke test before uploading artifacts:

```sh
devenv --version
devenv shim rehash
devenv doctor
```

Executable smoke runs only when the package target matches the build host target. Cross-compiled packages such as `aarch64-unknown-linux-gnu` on an `x86_64-unknown-linux-gnu` runner are still unpacked and checked for a non-empty binary, but the binary is not executed by default.

Control release smoke explicitly with:

```sh
DEVENV_RELEASE_SMOKE=auto scripts/package-release.sh
DEVENV_RELEASE_SMOKE=1 scripts/package-release.sh
DEVENV_RELEASE_SMOKE=0 scripts/package-release.sh
```

The version output includes target, build profile, and git sha:

```text
devenv <version> (target=aarch64-apple-darwin, profile=release, git=<sha>)
```

Build timestamps are intentionally omitted so artifacts stay reproducible enough for early users.

## Network, Cache, And Offline Behavior

DevEnv separates metadata refresh from runtime artifact download.

- `devenv metadata update <tool>` and `devenv list-remote <tool> --refresh` fetch small provider metadata and write `$DEVENV_HOME/cache/metadata/<tool>/<provider>/metadata.json`.
- `devenv list-remote <tool> --offline` reads fixture overrides or the metadata cache and does not use the network.
- `devenv install <tool> <version>` downloads the selected runtime artifact only after metadata resolution.
- downloaded artifacts are cached under `$DEVENV_HOME/cache/downloads`.

Checksums are part of the Direct provider contract. A checksum-bearing artifact is promoted into the download cache only after verification. Providers without usable checksums should not be enabled for default Direct install.

## Metadata Catalog Distribution

DevEnv also has an experimental GitHub metadata catalog distribution path. The catalog is metadata only: it contains manifest files and normalized provider payloads with artifact URLs, checksums, platform mappings, and selector metadata. It must not contain Go, Java, Node.js, Flutter, Terraform, OpenTofu, or other runtime artifact archives.

Catalog release assets are separate from DevEnv binary release artifacts. Candidate metadata release assets are:

```text
devenv-catalog-v1-<catalog_version>.tar.gz
manifest.json
manifest.sig
manifest.cert
SHA256SUMS
```

Trust policy:

- The catalog manifest must be verified against a pinned trust root before payloads are written to the metadata cache.
- Manifest signature verification is the authenticity check. Adjacent checksum files are useful transfer diagnostics, but they do not replace signature verification.
- Manifest entries bind each normalized payload through `sha256:<hex>` payload checksums.
- Catalog trust failure is different from catalog network failure. Trust failures must not silently fall back to official provider refresh.
- Mutable raw branch URLs are not default distribution sources. Default catalog distribution should use immutable GitHub Release assets or immutable tag-based URLs.

Catalog rollout:

- Experimental: users opt in with `--source catalog`, explicit catalog file/base URL, or `DEVENV_ENABLE_CATALOG=1`.
- Beta: selected providers may include catalog in `--source auto` after smoke tests and signature verification are stable.
- Stable: catalog can become the preferred metadata source only after publishing, signing, mirror, rollback, and support procedures have been exercised.

For organization mirrors and air-gapped environments, prefer source overrides rather than project-specific URLs:

- verified catalog release archives such as `devenv-catalog-v1-<catalog_version>.tar.gz`, extracted to a file or HTTP mirror;
- normalized fixture variables such as `DEVENV_GO_RELEASE_METADATA`;
- official-payload fixtures such as `DEVENV_GO_OFFICIAL_RELEASE_METADATA`;
- provider mirror base URLs such as `DEVENV_NODE_OFFICIAL_BASE_URL`, `DEVENV_FLUTTER_OFFICIAL_BASE_URL`, `DEVENV_TERRAFORM_OFFICIAL_BASE_URL`, and `DEVENV_OPENTOFU_OFFICIAL_BASE_URL`.

Project files should record versions. Provider and mirror configuration should remain user/global operational configuration.

Catalog mirror example:

```sh
# connected staging machine
devenv metadata verify-catalog go --catalog /staging/devenv-metadata/v1 --source file

# air-gapped machine
export DEVENV_ENABLE_CATALOG=1
export DEVENV_CATALOG_BASE_URL=file:///mirror/devenv-metadata/v1
devenv metadata update go --source catalog
devenv list-remote go --offline
```

## Opt-In Network Smoke

The default test suite is offline. Real provider smoke checks are intentionally opt-in:

```sh
scripts/network-smoke.sh --help
DEVENV_NETWORK_SMOKE=1 scripts/network-smoke.sh
```

The default network smoke refreshes Go official metadata and verifies that `list-remote go --offline` can read the refreshed cache. It does not download a runtime artifact.

An actual artifact download smoke is separated behind a second flag:

```sh
DEVENV_NETWORK_SMOKE=1 DEVENV_NETWORK_SMOKE_DOWNLOAD=1 scripts/network-smoke.sh
```

Do not make this script a required CI gate without an explicit network-enabled job. Network provider failures should not block the normal offline development loop.

The catalog smoke is separate because it verifies the DevEnv-managed catalog layer instead of upstream provider metadata:

```sh
scripts/catalog-smoke.sh --help
DEVENV_CATALOG_SMOKE=1 \
DEVENV_CATALOG_SMOKE_BASE_URL=https://github.com/<org>/devenv-metadata/releases/download/catalog-2026.05.23.1/v1 \
scripts/catalog-smoke.sh
```

The default catalog smoke verifies the manifest and Go catalog payload, refreshes Go metadata from the catalog source, and checks that `list-remote go --offline` can read the refreshed catalog cache. It does not download a runtime artifact.

An actual artifact download smoke remains behind a second flag:

```sh
DEVENV_CATALOG_SMOKE=1 \
DEVENV_CATALOG_SMOKE_DOWNLOAD=1 \
DEVENV_CATALOG_SMOKE_BASE_URL=https://github.com/<org>/devenv-metadata/releases/download/catalog-2026.05.23.1/v1 \
scripts/catalog-smoke.sh
```

Do not make `scripts/catalog-smoke.sh` a required CI gate unless the job is explicitly network-enabled and points at an immutable catalog release or controlled mirror. The normal development and CI loop must remain offline by default.

## Release Version Bump

The product version is owned by `workspace.package.version` in the root `Cargo.toml`. Do not maintain a separate Rust constant for the CLI version. Release packaging, Git tags, Homebrew formula templates, and npm package manifests should derive from this version.

Prepare a release version:

```sh
scripts/release-version.sh <version>
```

By default the script requires a clean worktree, updates `Cargo.toml`, refreshes `Cargo.lock` through `cargo test`, commits `Cargo.toml` and `Cargo.lock` as `Release <version>`, and creates the annotated tag `v<version>`.

The script does not push commits or tags. Push the release commit and tag explicitly after review:

```sh
git push origin HEAD --tags
```

Useful variants:

```sh
scripts/release-version.sh <version> --dry-run
scripts/release-version.sh <version> --no-commit
scripts/release-version.sh <version> --no-tag
```

npm packaging must read the same Cargo workspace version when generating `@sponzey/devenv` and platform package `package.json` files. Do not hand-edit npm package versions separately.

## Local Packaging

Package the host target locally:

```sh
DEVENV_BUILD_GIT_SHA="$(git rev-parse --short=12 HEAD)" scripts/package-release.sh
```

Package an explicit target:

```sh
DEVENV_BUILD_GIT_SHA="$(git rev-parse --short=12 HEAD)" \
DEVENV_RELEASE_TARGET=aarch64-apple-darwin \
scripts/package-release.sh
```

Linux arm64 cross packaging uses:

```sh
DEVENV_USE_CROSS=1 DEVENV_RELEASE_TARGET=aarch64-unknown-linux-gnu scripts/package-release.sh
```

## Checksums

Per-artifact checksum files are generated next to each archive. GitHub releases also include `SHA256SUMS`.

Verify an artifact:

```sh
cd target/dist
shasum -a 256 -c devenv-<version>-aarch64-apple-darwin.tar.gz.sha256
```

## Homebrew Tap

Start with a private or organization tap:

```sh
brew tap <org>/tap
brew install <org>/tap/devenv
```

Keep the formula in the tap repository at:

```text
Formula/devenv.rb
```

Use `packaging/homebrew/devenv.rb.template` from this repository as the source template. Replace:

- `{{GITHUB_OWNER}}`
- `{{GITHUB_REPO}}`
- `{{VERSION}}`
- `{{SHA256_MACOS_ARM64}}`
- `{{SHA256_MACOS_X64}}`

The formula installs release artifacts directly, so users do not need Rust installed.

Formula smoke test:

```sh
brew install <org>/tap/devenv
devenv --version
devenv doctor
brew test <org>/tap/devenv
```

Do not submit to `homebrew/core` until DevEnv has stable releases, public adoption, and a support policy for formula updates.

## npm

The public npm package name is:

```text
@sponzey/devenv
```

The npm package is generated from the root Cargo workspace version. Do not keep a separate npm version field in source control. Generate the package with:

```sh
scripts/package-npm.sh
```

The generated package lives at:

```text
target/npm/@sponzey/devenv
```

Run the offline npm packaging smoke before publishing:

```sh
scripts/npm-smoke.sh
```

Publish the package only after the matching GitHub release artifacts are available. The npm `postinstall` script downloads:

```text
https://github.com/Sponzey-com/DevEnv/releases/download/v<version>/devenv-<version>-<target>.tar.gz
https://github.com/Sponzey-com/DevEnv/releases/download/v<version>/devenv-<version>-<target>.tar.gz.sha256
```

Manual publish:

```sh
scripts/package-npm.sh
npm login --registry=https://registry.npmjs.org/
npm publish target/npm/@sponzey/devenv --access public --registry=https://registry.npmjs.org/
```

First publish bootstrap:

- use the official npm registry, `https://registry.npmjs.org/`;
- do not use CNPM, npmmirror, or another mirror for account creation or publish;
- create or confirm the npm organization/user scope `sponzey`;
- confirm the publishing npm account has permission to create packages under `@sponzey`;
- publish once from an authorized account with 2FA, or use a granular access token with write permission and Bypass 2FA enabled;
- after the package exists, configure Trusted Publishing in the package settings for future releases.

If npm fails with `E404 Not Found - PUT https://registry.npmjs.org/@sponzey%2fdevenv`, treat it as a scope/package access problem: the npm publisher cannot see or create `@sponzey/devenv`.

GitHub Actions publish:

- set repository variable `NPM_PUBLISH_ENABLED` to `true` only after the npm scope/package can be published;
- leave `NPM_PUBLISH_ENABLED` unset while bootstrapping npm so GitHub Release artifact publishing is not blocked by npm registry permissions;
- configure npm Trusted Publishing for package `@sponzey/devenv`;
- publisher: GitHub Actions;
- organization or user: `Sponzey-com`;
- repository: `DevEnv`;
- workflow filename: `release.yml`;
- allowed action: `npm publish`;
- push the release tag;
- the release workflow creates GitHub release assets first, then publishes `@sponzey/devenv`;
- the workflow uses GitHub OIDC through `id-token: write` and does not require a long-lived `NPM_TOKEN`.

If token-based publishing is used instead of Trusted Publishing, the token must be a granular access token with write access to the package or scope and Bypass 2FA enabled. A regular token will fail with `E403` when npm requires two-factor authentication for package publishing.

User install/update:

```sh
npm install -g @sponzey/devenv@latest
```

If npm returns `404 Not Found`, either `@sponzey/devenv` has not been published to the public registry yet, or the requested version does not exist. Publishing a GitHub Release alone does not create an npm package.

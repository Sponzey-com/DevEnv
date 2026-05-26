# DevEnv

[Korean](README.md.ko)

DevEnv is a Rust-based CLI for selecting, installing, and activating development runtimes and command-line tools per project.

The project takes inspiration from tools such as `jenv`, `goenv`, `pyenv`, `asdf`, and `mise`, while aiming for a broader, extensible provider model across many languages and tools. DevEnv is CLI-first and does not require a server, GUI, daemon, database, or cloud dependency.

The current product version is `0.1.13`. The single source of truth is the root `Cargo.toml`:

```text
Cargo.toml -> [workspace.package] -> version
```

## Current Status

DevEnv is currently in an MVP stage. The core workflow is implemented, but providers do not all have the same maturity level.

Supported workflows:

- Register an existing runtime: `devenv add <tool> <path>`
- Remove a registered external runtime: `devenv remove <tool>@<version>` or `devenv remove <tool> <path>`
- Delete a DevEnv-owned install: `devenv uninstall <tool>@<version>`
- List installed and registered runtimes: `devenv list <tool>`
- List remote versions: `devenv list-remote <tool>`
- Install a runtime: `devenv install <tool>@<version>`
- Select a project version: `devenv local <tool>@<version>`
- Select a global version: `devenv global <tool>@<version>`
- Show the current selection: `devenv current`
- Run a command in an activated environment: `devenv exec -- <command>`
- Generate shims: `devenv shim rehash`
- Check local state: `devenv doctor`

Provider status:

| Tool         | Provider       | Current support                                                              |
| ------------ | -------------- | ---------------------------------------------------------------------------- |
| Java         | Temurin        | Direct install metadata and local registration                               |
| Go           | Official       | Direct install metadata, catalog metadata, local registration                |
| Node.js      | Official       | Direct install metadata, catalog metadata, local registration                |
| Python       | CPython        | Fixture-backed direct path and local registration; live provider is deferred |
| Ruby         | Local          | Local registration only                                                      |
| PHP          | Local          | Local registration only                                                      |
| Rust         | rustup         | Delegated to rustup; DevEnv discovers/registers toolchains                   |
| Flutter/Dart | Stable channel | Direct install metadata and local registration                               |
| Terraform    | HashiCorp      | Direct install metadata, catalog path, single-binary install                 |
| OpenTofu     | OpenTofu       | Direct install metadata, single-binary install                               |

DevEnv reads common project version files where implemented:

- `devenv.toml`
- `.tool-versions`
- `.java-version`
- `.go-version`
- `.node-version`
- `.nvmrc`
- `.python-version`
- `.ruby-version`

More details are in `docs/user-guide.md`.

## Quick Start From Source

Build and run locally:

```sh
cargo run --bin devenv -- --help
cargo run --bin devenv -- --version
```

Run tests:

```sh
cargo test
```

Build a release binary for the host target:

```sh
cargo build --release --bin devenv
```

Use a built binary:

```sh
target/release/devenv --version
target/release/devenv doctor
```

## Example Usage

Register runtimes that already exist on the machine:

```sh
devenv add java /Library/Java/JavaVirtualMachines/temurin-17.jdk/Contents/Home
devenv add go /usr/local/go
devenv add node ~/.nvm/versions/node/v20.11.1
devenv add python ~/.pyenv/versions/3.12.2
devenv add rust ~/.rustup/toolchains/1.85.0-aarch64-apple-darwin
```

Select project versions:

```sh
devenv local java@17
devenv local go@1.22
devenv local node@20
```

Activate and run commands:

```sh
devenv current
devenv exec -- java -version
devenv exec -- go version
devenv exec -- node --version
```

Refresh remote metadata and install:

```sh
devenv list-remote go --refresh
devenv install go@1.22
```

## Metadata And Catalog

DevEnv separates metadata refresh from runtime artifact downloads.

- `metadata update` and `list-remote --refresh` fetch small provider metadata.
- `list-remote --offline` reads fixture overrides or the local metadata cache.
- `install` downloads runtime artifacts only after metadata resolution.
- checksum-bearing artifacts are verified before being promoted into the download cache.

The GitHub metadata catalog path is experimental. The catalog stores normalized metadata only. It must not store runtime archives.

Current catalog support includes:

- schema v1 documents and fixtures;
- manifest and payload checksum verification;
- Go catalog metadata path;
- Node catalog metadata path;
- Terraform/OpenTofu catalog shape validation;
- opt-in catalog smoke script.

Catalog usage is opt-in:

```sh
export DEVENV_ENABLE_CATALOG=1
export DEVENV_CATALOG_BASE_URL=file:///mirror/devenv-metadata/v1

devenv metadata verify-catalog go --catalog /mirror/devenv-metadata/v1 --source file
devenv metadata update go --source catalog
devenv list-remote go --offline
```

Catalog network smoke is not part of the default test loop:

```sh
scripts/catalog-smoke.sh --help
DEVENV_CATALOG_SMOKE=1 DEVENV_CATALOG_SMOKE_BASE_URL=file:///mirror/devenv-metadata/v1 scripts/catalog-smoke.sh
```

## Distribution

Distribution currently uses GitHub release artifacts and npm.

### Release Version

The product version is controlled only by:

```text
Cargo.toml -> [workspace.package] -> version
```

Do not maintain a separate Rust, npm, or documentation version.

Prepare a version bump:

```sh
scripts/release-version.sh <version>
```

The script updates `Cargo.toml`, refreshes `Cargo.lock`, runs verification, creates a `Release <version>` commit, and creates an annotated `v<version>` tag. It does not push commits or tags.

Push explicitly after review:

```sh
git push origin HEAD --tags
```

### GitHub Release Artifacts

The release workflow builds these targets:

- `aarch64-apple-darwin`
- `x86_64-apple-darwin`
- `x86_64-unknown-linux-gnu`
- `aarch64-unknown-linux-gnu`
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

Package smoke executes the binary only when the package target matches the build host. Cross-compiled artifacts such as `aarch64-unknown-linux-gnu` are unpacked and checked, but not executed by default.

Build a local package for the host target:

```sh
DEVENV_BUILD_GIT_SHA="$(git rev-parse --short=12 HEAD)" scripts/package-release.sh
```

Build an explicit target:

```sh
DEVENV_BUILD_GIT_SHA="$(git rev-parse --short=12 HEAD)" \
DEVENV_RELEASE_TARGET=aarch64-apple-darwin \
scripts/package-release.sh
```

### npm

The public npm package name is:

```text
@sponzey/devenv
```

The npm package is generated from the Cargo workspace version. Do not hand-edit an npm package version.

Generate and smoke-test the package:

```sh
scripts/npm-smoke.sh
```

Publish after the matching GitHub release artifacts are available:

```sh
scripts/package-npm.sh
npm login --registry=https://registry.npmjs.org/
npm publish target/npm/@sponzey/devenv --access public --registry=https://registry.npmjs.org/
```

The release workflow publishes through npm Trusted Publishing, not a long-lived `NPM_TOKEN`. Configure npm with GitHub Actions as a trusted publisher for package `@sponzey/devenv`, repository `Sponzey-com/DevEnv`, workflow filename `release.yml`, and allowed action `npm publish`.

Use the official npm registry for account creation and publishing. CNPM, npmmirror, and other mirrors are not valid places to create or publish the public `@sponzey/devenv` package.

The release workflow only attempts npm publish when repository variable `NPM_PUBLISH_ENABLED` is set to `true`. Leave it unset while bootstrapping the npm scope so GitHub Release artifacts can still be published.

For the first publish, make sure the npm `sponzey` organization or user scope exists and that the publishing account has permission to create `@sponzey/devenv`. If npm returns `E404` during `PUT https://registry.npmjs.org/@sponzey%2fdevenv`, the package or scope is not accessible to the publisher. Bootstrap the package once with an authorized npm account using 2FA or a granular access token with Bypass 2FA, then enable Trusted Publishing for subsequent releases.

The package installs a small Node.js shim and downloads the matching prebuilt GitHub release artifact during `postinstall`. It verifies the `.tar.gz.sha256` checksum before installing the local `devenv` binary.

User-facing install/update:

```sh
npm install -g @sponzey/devenv@latest
```

If npm returns `404 Not Found`, the package name has not been published yet or the requested version is not available in the public registry.

## Development Standards

The project is developed around:

- Clean Architecture
- Tidy First
- TDD

Core policy should stay independent from shell, filesystem, network, package registry, archive format, and platform-specific details. New languages and tools should be added through stable contracts, adapters, and tests rather than hard-coded behavior in the CLI layer.

## Documentation

- User guide: `docs/user-guide.md`
- Distribution guide: `docs/distribution.md`
- Catalog schema: `docs/catalog/schema-v1.md`
- Catalog repository workflow: `docs/catalog/repository-workflow.md`
- Architecture decisions: `docs/adr/`

## License

MIT. See `LICENSE`.

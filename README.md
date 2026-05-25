# DevEnv

DevEnv는 여러 개발 런타임과 CLI 도구를 한 프로젝트 안에서 선택, 설치, 활성화하기 위한 Rust 기반 CLI입니다.

목표는 `jenv`, `goenv`, `pyenv`, `asdf`, `mise` 계열의 장점을 참고하되, 더 많은 언어와 도구를 장기적으로 확장 가능한 구조로 지원하는 것입니다. 현재 구현은 CLI 중심이며, 서버, GUI, daemon, database, cloud dependency를 요구하지 않습니다.

현재 제품 버전은 root `Cargo.toml`의 `workspace.package.version`을 기준으로 합니다.

## Current Status

DevEnv는 현재 MVP 단계입니다. 핵심 흐름은 동작하지만, 모든 provider가 같은 성숙도를 가진 것은 아닙니다.

지원 중인 기본 워크플로:

- 기존 런타임 등록: `devenv add <tool> <path>`
- 등록 해제: `devenv remove <tool>@<version>` 또는 `devenv remove <tool> <path>`
- DevEnv 소유 설치 삭제: `devenv uninstall <tool>@<version>`
- 설치된/등록된 런타임 조회: `devenv list <tool>`
- 원격 버전 조회: `devenv list-remote <tool>`
- 런타임 설치: `devenv install <tool>@<version>`
- 프로젝트 선택: `devenv local <tool>@<version>`
- 전역 선택: `devenv global <tool>@<version>`
- 현재 선택 확인: `devenv current`
- 활성 환경에서 명령 실행: `devenv exec -- <command>`
- shim 생성: `devenv shim rehash`
- 상태 점검: `devenv doctor`

Provider 상태:

| Tool | Provider | Current support |
| --- | --- | --- |
| Java | Temurin | Direct install metadata and local registration |
| Go | Official | Direct install metadata, catalog metadata, local registration |
| Node.js | Official | Direct install metadata, catalog metadata, local registration |
| Python | CPython | Fixture-backed direct path and local registration; live provider is deferred |
| Ruby | Local | Local registration only |
| PHP | Local | Local registration only |
| Rust | rustup | Delegated to rustup; DevEnv discovers/registers toolchains |
| Flutter/Dart | Stable channel | Direct install metadata and local registration |
| Terraform | HashiCorp | Direct install metadata, catalog path, single-binary install |
| OpenTofu | OpenTofu | Direct install metadata, single-binary install |

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

## Metadata And Catalog Status

DevEnv separates metadata refresh from runtime artifact downloads.

- `metadata update` and `list-remote --refresh` fetch small provider metadata.
- `list-remote --offline` reads fixture overrides or local metadata cache.
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

Distribution currently starts with GitHub release artifacts and an organization Homebrew tap. Direct submission to `homebrew/core` is a later goal.

### Release Version

The product version is controlled only by:

```text
Cargo.toml -> [workspace.package] -> version
```

Do not maintain a separate Rust, npm, Homebrew, or documentation version.

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

### Homebrew

Homebrew distribution is expected to start through an organization tap:

```sh
brew tap <org>/tap
brew install <org>/tap/devenv
```

The formula template lives at:

```text
packaging/homebrew/devenv.rb.template
```

The formula installs prebuilt GitHub release artifacts, so users do not need Rust installed.

### npm

npm is an intended distribution channel for DevEnv. The package name is:

```text
@sponzey/devenv
```

Package structure:

- `@sponzey/devenv` as the thin meta package with the `devenv` bin shim;
- `@sponzey/devenv-darwin-arm64`, `@sponzey/devenv-darwin-x64`, `@sponzey/devenv-linux-x64`, `@sponzey/devenv-linux-arm64`, and `@sponzey/devenv-win32-x64` as platform packages;
- all npm package versions generated from the Cargo workspace version.

User-facing install/update:

```sh
npm install -g @sponzey/devenv@latest
```

Implementation status: npm package generation and publish automation still need to be added to the repository. The distribution target itself is not deferred.

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

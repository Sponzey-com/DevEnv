# DevEnv User Guide

This guide covers the Java, Go, Node.js, Python, Ruby, PHP, Rust, Flutter/Dart, Terraform, and OpenTofu MVP workflows.

## Concepts

DevEnv separates three ideas:

- `add`: register an existing runtime that DevEnv does not own.
- `install`: install a runtime into DevEnv-owned storage.
- `uninstall`: delete a runtime from DevEnv-owned storage.
- `local`, `global`, `shell`: select which version should be active.

The active version is resolved from highest to lowest precedence:

1. CLI override, such as `devenv current java 17`.
2. Shell selection from `devenv shell <tool> <version>`.
3. Project `devenv.toml`.
4. Global config from `DEVENV_GLOBAL_CONFIG`.

## Add Existing Runtimes

Register a JDK, Go SDK, Node.js runtime, Python runtime, Ruby runtime, PHP runtime, Rust toolchain, Flutter SDK, or single-binary IaC tool already installed on the machine:

```sh
devenv add java /Library/Java/JavaVirtualMachines/temurin-17.jdk/Contents/Home
devenv add go /usr/local/go
devenv add node ~/.nvm/versions/node/v20.11.1
devenv add python ~/.pyenv/versions/3.12.2
devenv add ruby ~/.rbenv/versions/3.3.0
devenv add php ~/.phpenv/versions/8.3.7
devenv add rust ~/.rustup/toolchains/1.85.0-aarch64-apple-darwin
devenv add flutter ~/sdks/flutter
devenv add terraform /opt/terraform-1.8.5/terraform
devenv add opentofu /opt/opentofu-1.8.5/tofu
```

List registered, discovered, and DevEnv-installed runtimes:

```sh
devenv list java
devenv list go
devenv list node
devenv list python
devenv list ruby
devenv list php
devenv list rust
devenv list flutter
devenv list terraform
devenv list opentofu
```

Remove an external runtime registration without deleting the runtime directory:

```sh
devenv remove java@17.0.11
devenv remove go /usr/local/go
devenv remove node ~/.nvm/versions/node/v20.11.1
devenv remove python ~/.pyenv/versions/3.12.2
devenv remove ruby ~/.rbenv/versions/3.3.0
devenv remove php ~/.phpenv/versions/8.3.7
devenv remove rust ~/.rustup/toolchains/1.85.0-aarch64-apple-darwin
devenv remove flutter ~/sdks/flutter
devenv remove terraform /opt/terraform-1.8.5/terraform
devenv remove opentofu /opt/opentofu-1.8.5/tofu
```

`remove` is intentionally limited to external registrations. It does not delete runtime files.

## Select Versions

Project-local selection writes `devenv.toml` in the current directory:

```sh
devenv local java 17
devenv local go 1.22
devenv local node 20
devenv local python 3.12
devenv local ruby 3.3
devenv local php 8.3
devenv local rust 1.85
devenv local flutter 3.24
devenv local terraform 1.8
devenv local opentofu 1.8
```

Global selection writes the file pointed to by `DEVENV_GLOBAL_CONFIG`, or the default DevEnv global config when that variable is not set:

```sh
export DEVENV_GLOBAL_CONFIG="$HOME/.config/devenv/devenv.toml"
devenv global java 21
```

When `DEVENV_GLOBAL_CONFIG` is not set, DevEnv writes the global selection to the default global config under `DEVENV_HOME`.

Shell selection prints an export command and does not write files:

```sh
eval "$(devenv shell java 17)"
```

Inspect the active selection:

```sh
devenv current
devenv current java
devenv current go 1.22.5
devenv current node
devenv current python
devenv current ruby
devenv current php
devenv current rust
devenv current flutter
devenv current terraform
devenv current opentofu
```

Project config compatibility files are read in this order when present:

- `devenv.toml`
- `.tool-versions`
- `.java-version`
- `.go-version`
- `.node-version`
- `.nvmrc`
- `.python-version`
- `.ruby-version`

`.node-version` and `.nvmrc` select the Node.js runtime only. `package.json` fields such as `packageManager`, npm, pnpm, and yarn pinning are intentionally not interpreted yet.
`.python-version` selects the Python runtime only. Virtual environments, `pyproject.toml`, `uv.lock`, conda, and pixi files are intentionally not interpreted yet.
`.ruby-version` selects the Ruby runtime only. Gemsets, `.ruby-gemset`, Bundler lockfiles, and Ruby source-build metadata are intentionally not interpreted yet.
Rust is selected through `devenv.toml`, `.tool-versions`, shell scope, or CLI override. DevEnv does not interpret `rust-toolchain` or `rust-toolchain.toml` yet.

## Execute Commands

Run a command with all selected tools activated:

```sh
devenv exec -- java -version
devenv exec -- go version
devenv exec -- node --version
devenv exec -- python --version
devenv exec -- ruby --version
devenv exec -- php --version
devenv exec -- rustc --version
devenv exec -- flutter --version
devenv exec -- terraform version
devenv exec -- tofu version
```

`devenv exec` sets tool-specific environment such as `JAVA_HOME`, `GOROOT`, and `FLUTTER_ROOT`, then prepends the selected runtime directories to `PATH`. Node.js, Python, Ruby, PHP, and Rust activation only prepend the runtime `bin` directory. Terraform and OpenTofu prepend the directory that contains the selected single binary.

## Ruby And PHP

Ruby and PHP support is local-first. DevEnv can register and switch existing runtimes, but it does not install Ruby or PHP yet because source builds require platform packages, compilation flags, OpenSSL/readline/libyaml/zlib variants, PHP extension choices, and other host-specific decisions.

DevEnv can discover Ruby runtimes from:

- paths registered with `devenv add ruby <path>`;
- paths listed in `DEVENV_RUBY_CANDIDATE_PATHS`.

DevEnv can discover PHP runtimes from:

- paths registered with `devenv add php <path>`;
- paths listed in `DEVENV_PHP_CANDIDATE_PATHS`.

Ruby version detection is file-system based. A Ruby runtime should contain `bin/ruby`, `bin/gem`, and version metadata in `VERSION`, `.ruby-version`, `include/ruby-*/ruby/version.h`, or a versioned runtime directory name. DevEnv exposes `ruby`, `gem`, and `bundle` shims.

PHP version detection is file-system based. A PHP runtime should contain `bin/php`, `bin/phpize`, `bin/php-config`, and version metadata in `VERSION`, `include/main/php_version.h`, or a versioned runtime directory name. DevEnv exposes `php`, `phpize`, and `php-config` shims.

Remote Ruby/PHP install is intentionally deferred. The current strategy is recorded in `docs/adr/0006-ruby-php-install-strategy.md`.

## Flutter And Dart

Flutter is modeled as one SDK adapter named `flutter`. It exposes both `flutter` and `dart` shims from the selected Flutter SDK.

DevEnv can discover Flutter SDKs from:

- paths registered with `devenv add flutter <path>`;
- paths listed in `DEVENV_FLUTTER_CANDIDATE_PATHS`;
- DevEnv-owned installs from provider metadata.

Version detection is file-system based. A Flutter SDK should contain `bin/flutter`, `bin/dart`, and version metadata in `VERSION`, `version`, `bin/internal/flutter.version`, or a versioned SDK directory name.

Remote Flutter metadata uses the official stable channel release JSONs. `devenv list-remote flutter --refresh --channel stable` refreshes the metadata cache, and `--offline` reads that cache without network access. `--channel beta` and `--channel dev` are reserved for a later provider extension.

DevEnv does not generate Flutter projects, install Android Studio, install Xcode, CocoaPods, or manage Android/iOS platform toolchains in this phase.

## Terraform And OpenTofu

Terraform and OpenTofu validate the single-binary adapter shape. The tool names are `terraform` and `opentofu`; OpenTofu exposes the `tofu` binary.

DevEnv accepts either a direct binary path or a directory containing the binary:

```sh
devenv add terraform /usr/local/bin/terraform
devenv add opentofu /usr/local/bin/tofu
```

For external registration, version metadata should be available through `VERSION`, `.devenv-version`, `bin/VERSION`, or a versioned runtime directory name. DevEnv-owned installs use the selected install version and the generic plain-file extraction path.

Future IaC tools such as Terragrunt, Terramate, and Atmos are intentionally deferred until the Terraform/OpenTofu single-binary shape is stable.

## Rust And Rustup

Rust support intentionally coexists with rustup. DevEnv does not install Rust, run rustup, manage components, or manage cross-compilation targets in this phase.

DevEnv can discover Rust toolchains from:

- paths registered with `devenv add rust <path>`;
- paths listed in `DEVENV_RUST_CANDIDATE_PATHS`;
- `RUSTUP_HOME/toolchains` when `RUSTUP_HOME` is set.

Version detection is file-system based. A Rust toolchain should contain `bin/rustc`, `bin/cargo`, and either a `VERSION` file or a versioned rustup directory name such as `1.85.0-aarch64-apple-darwin`. Channel-style names such as `stable-aarch64-apple-darwin` require future rustup-aware discovery and are rejected with guidance instead of being guessed.

## Install Runtimes

### Provider Support Levels

DevEnv uses provider capabilities to describe what each tool can do.

| Tool | Provider | Support | Direct install status | Next action |
| --- | --- | --- | --- | --- |
| Go | official | Direct | Official metadata/cache path is supported. | Use `devenv list-remote go --refresh` or fixture metadata. |
| Java | temurin | Direct | Temurin metadata is supported. | Use `--distribution temurin`. |
| Node.js | official | Direct | Official index/checksum metadata is supported. | Use `devenv list-remote node --refresh`. |
| Flutter/Dart | stable | Direct | Stable channel metadata is supported. | Use `--channel stable`; beta/dev are deferred. |
| Terraform | hashicorp | Direct | Official release/checksum metadata is supported. | Use `devenv list-remote terraform --refresh`. |
| OpenTofu | opentofu | Direct | Official release/checksum metadata is supported. | Use `devenv list-remote opentofu --refresh`. |
| Python | cpython | Direct, fixture-backed | Live CPython direct install is deferred. | Use `DEVENV_PYTHON_RELEASE_METADATA` for fixtures or `devenv add python <path>`; see `docs/adr/0008-python-install-strategy.md`. |
| Rust | rustup | Delegated | DevEnv does not install Rust toolchains. | Use rustup, then let DevEnv discover `RUSTUP_HOME` or run `devenv add rust <path>`. |
| Ruby | local | LocalOnly | Remote install is deferred. | Register an existing runtime with `devenv add ruby <path>`. |
| PHP | local | LocalOnly | Remote install is deferred. | Register an existing runtime with `devenv add php <path>`. |

### Remote Metadata And Cache

Remote metadata is separate from runtime artifact downloads. `list-remote`, `metadata update`, and `metadata status` deal with small provider metadata. `install` is the command that downloads the selected runtime archive or binary.

DevEnv reads metadata in this order:

1. Explicit fixture or source override from environment.
2. Explicit catalog file/base URL override.
3. User/global mirror configuration or mirror environment variables.
4. Existing fresh DevEnv metadata cache.
5. Experimental DevEnv GitHub metadata catalog when enabled.
6. Official provider refresh when `--refresh` or `metadata update` asks for it.
7. Stale cache fallback when policy allows it.

`--offline` disables network catalog and official network refresh. Offline commands can still use fixture overrides, local file catalog sources, mirror file URLs, and cache entries.

Metadata cache entries are stored under:

```text
$DEVENV_HOME/cache/metadata/<tool>/<provider>/metadata.json
```

The cache file is DevEnv-owned state. Do not edit it by hand; seed metadata through fixture variables, mirror variables, or `devenv metadata update`.

Normalized fixture variables are still supported for tests, mirrors, and air-gapped environments:

```sh
export DEVENV_JAVA_RELEASE_METADATA=/path/to/java-releases.toml
export DEVENV_GO_RELEASE_METADATA=/path/to/go-releases.toml
export DEVENV_NODE_RELEASE_METADATA=/path/to/node-releases.toml
export DEVENV_PYTHON_RELEASE_METADATA=/path/to/python-releases.toml
export DEVENV_FLUTTER_RELEASE_METADATA=/path/to/flutter-releases.toml
export DEVENV_TERRAFORM_RELEASE_METADATA=/path/to/terraform-releases.toml
export DEVENV_OPENTOFU_RELEASE_METADATA=/path/to/opentofu-releases.toml
```

Some providers also accept official-style fixtures or mirror base URLs. These inputs use the provider parser and write the same metadata cache format:

```sh
export DEVENV_GO_OFFICIAL_RELEASE_METADATA=/path/to/go-official.json
export DEVENV_NODE_OFFICIAL_RELEASE_INDEX=/path/to/node-index.json
export DEVENV_NODE_OFFICIAL_SHASUMS_DIR=/path/to/node-shasums
export DEVENV_NODE_OFFICIAL_BASE_URL=https://mirror.example.com/nodejs
export DEVENV_FLUTTER_OFFICIAL_RELEASES_DIR=/path/to/flutter-release-jsons
export DEVENV_FLUTTER_OFFICIAL_BASE_URL=https://mirror.example.com/flutter
export DEVENV_TERRAFORM_OFFICIAL_RELEASE_INDEX=/path/to/terraform-releases.json
export DEVENV_TERRAFORM_OFFICIAL_SHA256SUMS_DIR=/path/to/terraform-shasums
export DEVENV_TERRAFORM_OFFICIAL_BASE_URL=https://mirror.example.com/terraform
export DEVENV_OPENTOFU_OFFICIAL_RELEASES=/path/to/opentofu-releases.json
export DEVENV_OPENTOFU_OFFICIAL_SHA256SUMS_DIR=/path/to/opentofu-shasums
export DEVENV_OPENTOFU_OFFICIAL_BASE_URL=https://mirror.example.com/opentofu
```

Java Temurin currently supports normalized Java fixture metadata and Temurin API JSON fixtures through `DEVENV_JAVA_TEMURIN_RELEASE_METADATA`. Live Temurin HTTP refresh is deferred.

Python is fixture-backed for install pipeline tests and local experiments. Live CPython Direct install is deferred by `docs/adr/0008-python-install-strategy.md`.

### GitHub Metadata Catalog

The DevEnv metadata catalog is an experimental normalized metadata mirror. It stores small metadata payloads such as runtime versions, artifact URLs, checksums, platform mappings, provider selectors, and yanked/deprecated policy. It does not store runtime artifact archives. Runtime archives are downloaded only by `devenv install` after metadata resolution.

Rollout state:

- Experimental: catalog is opt-in through `--source catalog`, an explicit local catalog path/URL, or `DEVENV_ENABLE_CATALOG=1` together with `DEVENV_CATALOG_BASE_URL`.
- Beta: catalog can be included in `--source auto` for selected providers after Go/Node catalog smoke tests and signature verification are stable.
- Stable: catalog becomes the preferred default only after release publishing, signing, trust root policy, mirror docs, and rollback procedures are exercised.

Opt-in catalog flow:

```sh
export DEVENV_ENABLE_CATALOG=1
export DEVENV_CATALOG_BASE_URL=file:///mirror/devenv-metadata/v1

devenv metadata verify-catalog go --catalog /mirror/devenv-metadata/v1 --source file
devenv metadata update go --source catalog
devenv metadata status go
devenv list-remote go --offline
devenv install go 1.23
```

`metadata verify-catalog` validates the manifest signature, catalog freshness, and payload checksums before you use the catalog as a source. Current local development fixtures use a simple digest-backed signature verifier; production catalog publishing is documented separately and must use the accepted trust policy before the catalog becomes a default source.

Maintainers can run a live catalog smoke separately from the default offline test suite:

```sh
scripts/catalog-smoke.sh --help
DEVENV_CATALOG_SMOKE=1 DEVENV_CATALOG_SMOKE_BASE_URL=file:///mirror/devenv-metadata/v1 scripts/catalog-smoke.sh
```

The smoke verifies the catalog and refreshes Go metadata without downloading a runtime artifact. Set `DEVENV_CATALOG_SMOKE_DOWNLOAD=1` only when the job is allowed to download and install a runtime archive.

Source selection:

```sh
devenv metadata update go --source auto
devenv metadata update go --source env
devenv metadata update go --source cache
devenv metadata update go --source catalog
devenv metadata update go --source official
```

- `auto` keeps fixture overrides and fresh cache ahead of network sources.
- `env` and `cache` do not use the network.
- `catalog` makes catalog failures visible instead of hiding them behind official provider fallback.
- `official` skips catalog and uses the provider's official refresh path when implemented.

Fixture overrides remain first-class. A configured `DEVENV_GO_RELEASE_METADATA`, `DEVENV_NODE_RELEASE_METADATA`, or other normalized fixture variable wins over catalog source selection. This keeps tests, internal mirrors, and controlled air-gapped workflows stable while catalog support rolls out.

List remote versions:

```sh
devenv list-remote java
devenv list-remote go
devenv list-remote go --refresh
devenv list-remote go --offline
devenv list-remote node
devenv list-remote python
devenv list-remote flutter --channel stable
devenv list-remote terraform
devenv list-remote opentofu
```

Inspect provider capability and metadata cache state:

```sh
devenv provider list
devenv provider info java
devenv provider info java temurin
devenv metadata status
devenv metadata status go
devenv metadata verify-catalog go --catalog ./v1 --source file
```

Refresh metadata cache:

```sh
devenv metadata update go
devenv metadata update go --source catalog
devenv metadata update --all
```

`provider list` shows whether a tool is `Direct`, `Delegated`, or `LocalOnly`, along with checksum policy and selector dimensions such as Java distribution, Flutter channel, or Python implementation. `provider info` also reports whether an experimental catalog metadata path exists for that provider. `metadata status` combines provider capability with cache state: `missing`, `fresh`, `stale`, or `corrupt`, and prints `metadata_source` as `env`, `cache`, `catalog`, `official`, `stale-cache`, or `missing` where that can be determined. `metadata verify-catalog` verifies a configured or local catalog manifest and payloads without adding a new top-level catalog command.

Ruby and PHP report `LocalOnly`, Rust reports `Delegated`, and Python reports fixture-backed Direct support with live provider selection deferred by ADR.

### Mirrors And Air-Gapped Use

Mirror and air-gapped flows should keep project version selection separate from provider source selection. Put selected versions in `devenv.toml`, `.tool-versions`, or shell scope. Put source overrides in environment or future user/global provider config.

Recommended mirror paths:

- verified DevEnv catalog release archives extracted to an internal file or HTTP mirror;
- normalized fixture files for simple internal catalogs;
- official-style fixture directories for parser-compatible mirrors;
- provider base URLs such as `DEVENV_NODE_OFFICIAL_BASE_URL`, `DEVENV_FLUTTER_OFFICIAL_BASE_URL`, `DEVENV_TERRAFORM_OFFICIAL_BASE_URL`, or `DEVENV_OPENTOFU_OFFICIAL_BASE_URL`.

For catalog-based air-gapped use, verify the catalog archive on an online machine, copy the extracted `v1/` directory to the internal mirror, then update metadata from a file URL:

```sh
# online or connected staging machine
devenv metadata verify-catalog go --catalog /staging/devenv-metadata/v1 --source file

# air-gapped machine
export DEVENV_ENABLE_CATALOG=1
export DEVENV_CATALOG_BASE_URL=file:///mirror/devenv-metadata/v1
devenv metadata update go --source catalog
devenv list-remote go --offline
```

For fixture-based air-gapped use, seed metadata before going offline:

```sh
DEVENV_GO_OFFICIAL_RELEASE_METADATA=/mirror/go/releases.json devenv metadata update go
devenv list-remote go --offline
```

Troubleshooting catalog sources:

- `catalog unavailable` means the configured catalog path or URL is missing or unreachable. Check `DEVENV_CATALOG_BASE_URL`, pass `--catalog <path-or-url>`, or use `--source official` if official provider fallback is acceptable.
- `catalog network failure` means DevEnv could not fetch the catalog over HTTP. In `auto`, this may fall back to official provider refresh when policy allows it; in `--source catalog`, it fails.
- `catalog trust failure` means signature, trust root, freshness, schema, or payload checksum verification failed. Do not ignore this or silently fall back. Use a newer signed catalog release, fix the mirror, or inspect the trust root configuration.
- `metadata_source=catalog` in `devenv metadata status <tool>` means the local cache was populated from a verified catalog payload. `catalog_version`, `manifest_sha256`, and `payload_sha256` are printed when present.

Artifact downloads use a separate cache under `$DEVENV_HOME/cache/downloads`. Checksum-bearing artifacts are promoted into checksum-addressed cache entries only after verification. Providers without usable checksums are not accepted for default Direct install unless a future opt-in policy defines that risk.

Default tests and normal documentation verification do not require network access. Real provider network checks live in `scripts/network-smoke.sh` and only run when `DEVENV_NETWORK_SMOKE=1` is set.

Install into DevEnv-owned storage:

```sh
devenv install java 17
devenv install go 1.22.5
devenv install node 20
devenv install python 3.12
devenv install flutter 3.24
devenv install terraform 1.8
devenv install opentofu 1.8
```

Delete a DevEnv-owned runtime from storage:

```sh
devenv uninstall java 17
devenv uninstall go 1.22
devenv uninstall node 20
devenv uninstall python 3.12
devenv uninstall flutter 3.24
devenv uninstall terraform 1.8
devenv uninstall opentofu 1.8
```

`uninstall` only removes DevEnv-owned installs for the current platform. It does not remove runtimes registered with `devenv add`; use `devenv remove <tool> <path>` for those.

Install errors include the metadata variable to inspect, expected local artifact URL support, and checksum guidance. Default tests and fixture workflows do not require network access.

Ruby and PHP are not part of remote install yet. Register existing runtimes with `devenv add ruby <path>` or `devenv add php <path>`.

## Shim Setup

`devenv exec -- <command>` activates selected tools for one command. Direct commands such as `java --version` require the DevEnv shim directory to be active in the current shell.

Generate shims manually:

```sh
devenv shim init
```

Print shell activation for the current session. `activate` also generates shims, so this is normally the only setup command needed for direct tool commands:

```sh
eval "$(devenv activate zsh)"
eval "$(devenv activate bash)"
```

Fish:

```fish
devenv activate fish | source
```

PowerShell:

```powershell
devenv activate powershell | Invoke-Expression
```

Refresh shims after adapter binary metadata changes:

```sh
devenv shim rehash
```

The shim directory is placed under `DEVENV_HOME/shims`. DevEnv prints activation scripts only; it does not mutate shell profile files.

After activation, `devenv local`, `devenv global`, and `devenv use` selections are resolved by shims on the next tool command in the same shell. Without activation, those commands only write selection config; they cannot modify the already-running parent shell's `PATH`.

Generated shims currently include:

- Java: `java`, `javac`, `jar`, `javadoc`
- Go: `go`, `gofmt`
- Flutter/Dart: `flutter`, `dart`
- Terraform: `terraform`
- OpenTofu: `tofu`
- Node.js: `node`, `npm`, `npx`, `corepack`
- Python: `python`, `python3`, `pip`
- Ruby: `ruby`, `gem`, `bundle`
- PHP: `php`, `phpize`, `php-config`
- Rust: `rustc`, `cargo`

## Diagnostics

Run:

```sh
devenv doctor
devenv doctor --json
```

Doctor checks:

- `DEVENV_HOME` resolution.
- install store readability.
- runtime registry readability.
- shim directory presence and expected shim files.
- project config discovery.
- global config readability when `DEVENV_GLOBAL_CONFIG` is set.

## Distribution

Release packaging, checksum publication, and Homebrew tap setup are documented in `docs/distribution.md`.

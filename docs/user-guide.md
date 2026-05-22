# DevEnv User Guide

This guide covers the Java, Go, Node.js, Python, Ruby, PHP, Rust, Flutter/Dart, Terraform, and OpenTofu MVP workflows.

## Concepts

DevEnv separates three ideas:

- `add`: register an existing runtime that DevEnv does not own.
- `install`: install a runtime into DevEnv-owned storage.
- `uninstall`: delete a runtime from DevEnv-owned storage.
- `local`, `global`, `shell`: select which version should be active.

The active version is resolved from highest to lowest precedence:

1. CLI override, such as `devenv current java@17`.
2. Shell selection from `devenv shell <tool>@<version>`.
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
devenv local java@17
devenv local go@1.22
devenv local node@20
devenv local python@3.12
devenv local ruby@3.3
devenv local php@8.3
devenv local rust@1.85
devenv local flutter@3.24
devenv local terraform@1.8
devenv local opentofu@1.8
```

Global selection writes the file pointed to by `DEVENV_GLOBAL_CONFIG`:

```sh
export DEVENV_GLOBAL_CONFIG="$HOME/.config/devenv/devenv.toml"
devenv global java@21
```

Shell selection prints an export command and does not write files:

```sh
eval "$(devenv shell java@17)"
```

Inspect the active selection:

```sh
devenv current
devenv current java
devenv current go@1.22.5
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
- DevEnv-owned installs from fixture-backed metadata.

Version detection is file-system based. A Flutter SDK should contain `bin/flutter`, `bin/dart`, and version metadata in `VERSION`, `version`, `bin/internal/flutter.version`, or a versioned SDK directory name.

DevEnv does not generate Flutter projects, install Android Studio, install Xcode, or manage Android/iOS platform toolchains in this phase.

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

Remote metadata is currently fixture-backed for offline development. Set the metadata source before using `list-remote` or `install`:

```sh
export DEVENV_JAVA_RELEASE_METADATA=/path/to/java-releases.toml
export DEVENV_GO_RELEASE_METADATA=/path/to/go-releases.toml
export DEVENV_NODE_RELEASE_METADATA=/path/to/node-releases.toml
export DEVENV_PYTHON_RELEASE_METADATA=/path/to/python-releases.toml
export DEVENV_FLUTTER_RELEASE_METADATA=/path/to/flutter-releases.toml
export DEVENV_TERRAFORM_RELEASE_METADATA=/path/to/terraform-releases.toml
export DEVENV_OPENTOFU_RELEASE_METADATA=/path/to/opentofu-releases.toml
```

List remote versions:

```sh
devenv list-remote java
devenv list-remote go
devenv list-remote node
devenv list-remote python
devenv list-remote flutter
devenv list-remote terraform
devenv list-remote opentofu
```

Install into DevEnv-owned storage:

```sh
devenv install java@17
devenv install go@1.22.5
devenv install node@20
devenv install python@3.12
devenv install flutter@3.24
devenv install terraform@1.8
devenv install opentofu@1.8
```

Delete a DevEnv-owned runtime from storage:

```sh
devenv uninstall java@17
devenv uninstall go@1.22
devenv uninstall node@20
devenv uninstall python@3.12
devenv uninstall flutter@3.24
devenv uninstall terraform@1.8
devenv uninstall opentofu@1.8
```

`uninstall` only removes DevEnv-owned installs for the current platform. It does not remove runtimes registered with `devenv add`; use `devenv remove <tool> <path>` for those.

Install errors include the metadata variable to inspect, expected local artifact URL support, and checksum guidance. Default tests and fixture workflows do not require network access.

Ruby and PHP are not part of remote install yet. Register existing runtimes with `devenv add ruby <path>` or `devenv add php <path>`.

## Shim Setup

Generate shims:

```sh
devenv shim init
```

Print shell activation for the current session:

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

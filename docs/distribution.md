# DevEnv Distribution

DevEnv distribution starts with GitHub release artifacts and an organization Homebrew tap. Direct `brew install devenv` through `homebrew/core` is a later goal.

## Release Artifacts

Create a tag:

```sh
git tag v0.1.0
git push origin v0.1.0
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

The version output includes target, build profile, and git sha:

```text
devenv 0.1.0 (target=aarch64-apple-darwin, profile=release, git=<sha>)
```

Build timestamps are intentionally omitted so artifacts stay reproducible enough for early users.

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
shasum -a 256 -c devenv-0.1.0-aarch64-apple-darwin.tar.gz.sha256
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

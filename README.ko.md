# DevEnv

[English](README.md)

DevEnv는 여러 개발 런타임과 CLI 도구를 프로젝트 단위로 선택, 설치, 활성화하기 위한 Rust 기반 CLI입니다.

목표는 `jenv`, `goenv`, `pyenv`, `asdf`, `mise` 계열의 장점을 참고하되, 더 많은 언어와 도구를 장기적으로 확장 가능한 provider 모델로 지원하는 것입니다. DevEnv는 CLI 중심이며 서버, GUI, daemon, database, cloud dependency를 요구하지 않습니다.

제품 버전의 단일 기준은 root `Cargo.toml`입니다.

```text
Cargo.toml -> [workspace.package] -> version
```

## 현재 상태

DevEnv는 현재 MVP 단계입니다. 핵심 흐름은 구현되어 있지만 모든 provider가 같은 성숙도를 가진 것은 아닙니다.

지원 중인 기본 워크플로:

- 기존 런타임 등록: `devenv add <tool> <path>`
- 외부 등록 런타임 제거: `devenv remove <tool> <path>`
- DevEnv 소유 설치 삭제: `devenv uninstall <tool> <version>`
- 설치된/등록된 런타임 조회: `devenv list <tool>`
- 원격 버전 조회: `devenv list-remote <tool>`
- 런타임 설치: `devenv install <tool> <version>`
- 프로젝트 버전 선택: `devenv local <tool> <version>`
- 전역 버전 선택: `devenv global <tool> <version>`
- 현재 선택 확인: `devenv current`
- 활성 환경에서 명령 실행: `devenv exec -- <command>`
- shim 생성: `devenv shim rehash`
- 로컬 상태 점검: `devenv doctor`

Provider 상태:

| Tool | Provider | 현재 지원 |
| --- | --- | --- |
| Java | Temurin | 직접 설치 metadata와 로컬 등록 |
| Go | Official | 직접 설치 metadata, catalog metadata, 로컬 등록 |
| Node.js | Official | 직접 설치 metadata, catalog metadata, 로컬 등록 |
| Python | CPython | fixture 기반 직접 경로와 로컬 등록, live provider는 보류 |
| Ruby | Local | 로컬 등록만 지원 |
| PHP | Local | 로컬 등록만 지원 |
| Rust | rustup | rustup에 위임, DevEnv는 toolchain discovery/registration 담당 |
| Flutter | Stable channel | 직접 설치 metadata와 로컬 등록, bundled `dart` 노출 |
| Dart | Bundle via Flutter | standalone Dart SDK provider는 아직 없음, Flutter에 포함된 Dart SDK 사용 |
| Terraform | Planned | 아직 공식 문서상 지원 전, 내부 single-binary 작업은 실험 단계 |
| OpenTofu | Planned | 아직 공식 문서상 지원 전, 내부 single-binary 작업은 실험 단계 |

구현된 범위에서 다음 프로젝트 버전 파일을 읽습니다.

- `devenv.toml`
- `.tool-versions`
- `.java-version`
- `.go-version`
- `.node-version`
- `.nvmrc`
- `.python-version`
- `.ruby-version`

자세한 내용은 `docs/user-guide.md`에 있습니다.

## 소스에서 시작하기

로컬에서 빌드하고 실행:

```sh
cargo run --bin devenv -- --help
cargo run --bin devenv -- --version
```

테스트 실행:

```sh
cargo test
```

현재 host target용 release binary 빌드:

```sh
cargo build --release --bin devenv
```

빌드된 binary 사용:

```sh
target/release/devenv --version
target/release/devenv doctor
```

## 사용 예시

이미 설치되어 있는 런타임 등록:

```sh
devenv add java /Library/Java/JavaVirtualMachines/temurin-17.jdk/Contents/Home
devenv add go /usr/local/go
devenv add node ~/.nvm/versions/node/v20.11.1
devenv add python ~/.pyenv/versions/3.12.2
devenv add rust ~/.rustup/toolchains/1.85.0-aarch64-apple-darwin
```

프로젝트 버전 선택:

```sh
devenv local java 17
devenv local go 1.22
devenv local node 20
```

DevEnv 활성화로 명령 실행:

```sh
devenv current
devenv exec -- java -version
devenv exec -- go version
devenv exec -- node --version
```

`java --version`처럼 직접 실행하는 명령도 DevEnv 선택을 따르게 하려면 현재 쉘에서 shim을 한 번 활성화해야 합니다.

```sh
eval "$(devenv activate zsh)"
eval "$(devenv activate bash)"
java --version
```

`devenv local`과 `devenv global`은 버전 선택값을 기록합니다. 부모 shell process의 `PATH`를 직접 바꾸지는 않습니다. `go version` 같은 직접 명령이 바뀌려면 DevEnv shim directory가 `PATH`의 앞쪽에 활성화되어 있어야 합니다.

이 구조는 `goenv`, `jenv`, `pyenv`, `rbenv`와 같은 방식입니다. `local`은 프로젝트 버전 파일을 쓰고, shell 초기화 설정이 shim을 통해 매 명령마다 그 파일을 읽게 합니다.

새 터미널 세션에도 적용하려면 `devenv local`, `devenv global`, `devenv use`가 출력하는 정확한 activation line을 shell profile에 추가해야 합니다.

```sh
devenv init bash --write
devenv init zsh --write

# 수동 대안:
devenv global java 21
# zsh:  출력되는 "new sessions:" 줄을 ~/.zshrc에 추가합니다.
# bash: 출력되는 "new sessions:" 줄을 ~/.bashrc에 추가합니다.
```

Ubuntu/bash 예시:

```sh
devenv init bash --write
source ~/.bashrc
hash -r
type -a go
```

`devenv init <shell>`은 shell profile에 들어갈 managed block을 미리 보여주며 파일을 수정하지 않습니다. 실제로 profile을 수정하려면 `--write`를 붙입니다. 기존 DevEnv init block은 중복 추가하지 않고 교체합니다.

첫 번째 `go` 항목은 DevEnv shim directory를 가리켜야 합니다. Linux에서는 보통 `~/.local/share/devenv/shims/go`입니다. 기존 system Go 경로가 여전히 먼저 나오면 `~/.bashrc`에서 DevEnv activation line을 기존 Go `PATH` 설정보다 아래쪽으로 옮기세요.

npm 설치에서는 npm entrypoint가 Node.js wrapper이기 때문에 이 차이가 중요합니다. 출력되는 줄은 native DevEnv binary 경로를 사용하므로 DevEnv shim이 `node`, `npm` 같은 명령까지 안전하게 관리할 수 있습니다.

활성화된 세션에서는 이후 `devenv local`, `devenv global`, `devenv use` 선택이 다음 tool 명령부터 shim에 바로 반영됩니다.
shim 대상 명령에 DevEnv 선택 버전이 없으면 DevEnv는 `PATH`의 다음 실제 명령으로 fallback합니다.

원격 metadata 갱신과 설치:

```sh
devenv list-remote go --refresh
devenv install go 1.22
```

## Metadata와 Catalog

DevEnv는 metadata 갱신과 런타임 artifact 다운로드를 분리합니다.

- `metadata update`와 `list-remote --refresh`는 작은 provider metadata를 가져옵니다.
- `list-remote --offline`은 fixture override 또는 로컬 metadata cache를 읽습니다.
- `install`은 metadata resolution 이후에만 런타임 artifact를 다운로드합니다.
- checksum이 있는 artifact는 검증 후 download cache에 반영됩니다.

GitHub metadata catalog 경로는 실험 단계입니다. catalog는 정규화된 metadata만 저장하며 런타임 archive를 저장하면 안 됩니다.

현재 catalog 지원 범위:

- schema v1 문서와 fixture;
- manifest와 payload checksum 검증;
- Go catalog metadata 경로;
- Node catalog metadata 경로;
- 실험적 Terraform/OpenTofu catalog shape 검증만 포함;
- opt-in catalog smoke script.

Catalog 사용은 opt-in입니다.

```sh
export DEVENV_ENABLE_CATALOG=1
export DEVENV_CATALOG_BASE_URL=file:///mirror/devenv-metadata/v1

devenv metadata verify-catalog go --catalog /mirror/devenv-metadata/v1 --source file
devenv metadata update go --source catalog
devenv list-remote go --offline
```

Catalog network smoke는 기본 테스트 루프에 포함되지 않습니다.

```sh
scripts/catalog-smoke.sh --help
DEVENV_CATALOG_SMOKE=1 DEVENV_CATALOG_SMOKE_BASE_URL=file:///mirror/devenv-metadata/v1 scripts/catalog-smoke.sh
```

## 배포

현재 배포는 GitHub release artifact와 npm을 기준으로 합니다.

### Release Version

제품 버전은 다음 위치에서만 관리합니다.

```text
Cargo.toml -> [workspace.package] -> version
```

Rust, npm, 문서용 버전을 별도로 유지하지 않습니다.

버전 변경 준비:

```sh
scripts/release-version.sh <version>
```

스크립트는 `Cargo.toml`을 수정하고, `Cargo.lock`을 갱신하고, 검증을 실행한 뒤 `Release <version>` commit과 annotated `v<version>` tag를 만듭니다. commit과 tag를 push하지는 않습니다.

검토 후 명시적으로 push합니다.

```sh
git push origin HEAD --tags
```

### GitHub Release Artifacts

Release workflow는 다음 target을 빌드합니다.

- `aarch64-apple-darwin`
- `x86_64-apple-darwin`
- `x86_64-unknown-linux-musl`
- `aarch64-unknown-linux-musl`
- `x86_64-pc-windows-msvc`

Artifact 이름:

```text
devenv-<version>-<target>.tar.gz
devenv-<version>-<target>.tar.gz.sha256
SHA256SUMS
```

각 package에는 다음 파일이 포함됩니다.

- `devenv` 또는 `devenv.exe`
- `USER_GUIDE.md`

Linux release artifact는 musl target을 사용합니다. 그래서 npm 설치 시 사용자 Linux 배포판의 glibc 버전에 의존하지 않습니다.

Package smoke는 package target이 build host와 일치할 때만 binary를 실행합니다. `aarch64-unknown-linux-musl` 같은 cross-compiled artifact는 압축 해제와 non-empty binary 여부를 확인하지만 기본적으로 실행하지 않습니다.

Host target용 local package 빌드:

```sh
DEVENV_BUILD_GIT_SHA="$(git rev-parse --short=12 HEAD)" scripts/package-release.sh
```

명시 target 빌드:

```sh
DEVENV_BUILD_GIT_SHA="$(git rev-parse --short=12 HEAD)" \
DEVENV_RELEASE_TARGET=aarch64-apple-darwin \
scripts/package-release.sh
```

### npm

공개 npm package 이름:

```text
@sponzey/devenv
```

npm package는 Cargo workspace version에서 생성합니다. npm package version을 손으로 따로 수정하지 않습니다.

Package 생성과 smoke test:

```sh
scripts/npm-smoke.sh
```

매칭되는 GitHub release artifact가 올라간 뒤 publish합니다.

```sh
scripts/package-npm.sh
npm login --registry=https://registry.npmjs.org/
npm publish target/npm/@sponzey/devenv --access public --registry=https://registry.npmjs.org/
```

Release workflow는 장기 `NPM_TOKEN`이 아니라 npm Trusted Publishing을 사용합니다. npm에서 package `@sponzey/devenv`, repository `Sponzey-com/DevEnv`, workflow filename `release.yml`, allowed action `npm publish`로 GitHub Actions trusted publisher를 설정해야 합니다.

계정 생성과 publish는 공식 npm registry를 사용해야 합니다. CNPM, npmmirror 같은 mirror에서는 public `@sponzey/devenv` package를 생성하거나 publish할 수 없습니다.

Release workflow는 repository variable `NPM_PUBLISH_ENABLED`가 `true`일 때만 npm publish를 시도합니다. npm scope를 bootstrap하는 동안에는 이 값을 설정하지 않으면 GitHub Release artifact 배포는 계속 성공할 수 있습니다.

첫 publish 전에는 npm의 `sponzey` organization 또는 user scope가 존재하고, publish 계정이 `@sponzey/devenv`를 생성할 권한을 가져야 합니다. `PUT https://registry.npmjs.org/@sponzey%2fdevenv` 단계에서 `E404`가 나오면 package 또는 scope가 publisher에게 보이지 않는 상태입니다. 2FA가 가능한 npm 계정이나 Bypass 2FA가 켜진 granular access token으로 package를 한 번 bootstrap publish한 뒤, 이후 release부터 Trusted Publishing을 사용합니다.

이 package는 작은 Node.js shim을 설치하고 `postinstall` 중 매칭되는 prebuilt GitHub release artifact를 다운로드합니다. 로컬 `devenv` binary를 설치하기 전에 `.tar.gz.sha256` checksum을 검증합니다.

사용자 설치/업데이트:

```sh
npm install -g @sponzey/devenv@latest
```

npm이 `404 Not Found`를 반환하면 package가 아직 publish되지 않았거나 요청한 버전이 public registry에 없는 상태입니다.

## 개발 기준

이 프로젝트는 다음 기준을 중심으로 개발합니다.

- Clean Architecture
- Tidy First
- TDD

핵심 정책은 shell, filesystem, network, package registry, archive format, platform-specific detail과 분리되어야 합니다. 새로운 언어와 도구는 CLI layer에 hard-code하지 않고 안정적인 contract, adapter, test를 통해 추가합니다.

## 문서

- User guide: `docs/user-guide.md`
- Distribution guide: `docs/distribution.md`
- Catalog schema: `docs/catalog/schema-v1.md`
- Catalog repository workflow: `docs/catalog/repository-workflow.md`
- Architecture decisions: `docs/adr/`

## License

MIT. 자세한 내용은 `LICENSE`를 확인하세요.

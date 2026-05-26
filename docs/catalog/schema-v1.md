# DevEnv Catalog Schema v1

작성일: 2026-05-22

## 목적

DevEnv catalog schema v1은 upstream provider에서 얻은 runtime release 정보를 DevEnv가 소비하기 쉬운 normalized metadata로 배포하기 위한 계약이다. catalog는 runtime artifact 자체를 저장하지 않고, artifact URL, checksum, platform, archive type, provider selector 같은 작은 metadata만 저장한다.

이 schema는 다음 요구를 만족해야 한다.

- DevEnv CLI 업데이트 없이 새 runtime 버전을 노출할 수 있다.
- manifest signature와 payload checksum 검증으로 metadata 무결성을 확인할 수 있다.
- checksum 없는 artifact, yanked release, deprecated release, expired catalog를 data shape로 표현할 수 있다.
- 기존 core domain의 `RemoteReleaseIndex`, `RemoteRelease`, `ResolvedArtifact`, `Artifact`, `Platform`으로 손실 없이 매핑할 수 있다.
- offline test와 사내 mirror 운영을 위해 fixture와 archive 형태로 복제할 수 있다.

## 파일 구조

권장 catalog v1 구조:

```text
v1/
  manifest.json
  tools/
    go/official/releases.json
    node/official/releases.json
```

이 저장소의 fixture는 같은 구조를 `fixtures/catalog/v1` 아래에 둔다.

```text
fixtures/catalog/v1/
  manifest.json
  go/official/releases.json
  node/official/releases.json
```

실제 catalog repository에서는 manifest entry의 `path`가 `tools/<tool>/<provider>/releases.json` 형태를 사용한다. 이 저장소 fixture에서는 파일 깊이를 줄이기 위해 `go/official/releases.json`처럼 상대 경로를 사용해도 된다. consumer는 manifest가 가리키는 relative path만 신뢰한다.

## Manifest Object

`manifest.json`은 catalog payload 목록과 검증 정보를 담는다.

필수 필드:

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `schema_version` | integer | yes | manifest schema version. v1은 `1`만 허용한다. |
| `catalog_id` | string | yes | catalog trust boundary 식별자. 공식 catalog는 `dev.devenv.catalog`를 사용한다. |
| `generated_at` | string | yes | RFC3339 UTC 생성 시각. |
| `expires_at` | string | yes | RFC3339 UTC 만료 시각. 만료된 catalog는 fresh source로 쓰지 않는다. |
| `catalog_version` | string | yes | 사람이 읽고 정렬할 수 있는 catalog release version. 예: `2026.05.22.1`. |
| `min_devenv_version` | string | yes | 이 catalog를 안전하게 해석할 수 있는 최소 DevEnv CLI version. |
| `sequence` | integer | yes | rollback 감지와 최신성 판단을 위한 단조 증가 번호. |
| `entries` | array | yes | tool/provider별 payload entry 목록. |

선택 필드:

| Field | Type | Description |
| --- | --- | --- |
| `metadata` | object | catalog publisher, source repository, release URL 같은 부가 정보. |

### Manifest Entry

필수 필드:

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `tool` | string | yes | DevEnv `ToolName`. 예: `go`, `node`. |
| `provider` | string | yes | DevEnv `ProviderId`. 예: `official`, `temurin`. |
| `path` | string | yes | manifest 파일 기준 payload relative path. |
| `sha256` | string | yes | payload bytes의 sha256 digest. 형식은 `sha256:<64 lowercase hex>`. |
| `payload_kind` | string | yes | v1에서는 `normalized-release-index`를 사용한다. |
| `ttl_seconds` | integer | yes | payload cache freshness TTL. |

선택 필드:

| Field | Type | Description |
| --- | --- | --- |
| `platforms` | array | payload가 포함하는 platform id 목록. 예: `linux-x64`. |
| `selector` | object | provider channel, distribution, implementation 같은 coarse selector. |
| `min_devenv_version` | string | entry 단위로 더 높은 CLI version이 필요할 때 사용한다. |

검증 규칙:

- `schema_version`은 `1`이어야 한다.
- `catalog_id`는 trust root 정책과 일치해야 한다.
- `expires_at`이 현재 시각보다 과거면 catalog source는 stale로 판단한다.
- `min_devenv_version`이 현재 CLI보다 높으면 actionable error를 반환한다.
- `sequence`는 같은 catalog line에서 이전에 신뢰한 sequence보다 낮으면 rollback 후보로 판단한다.
- `entries[].sha256`은 payload file bytes와 정확히 일치해야 한다.
- manifest entry에 없는 payload는 cache에 쓰지 않는다.
- payload 내부의 `tool`과 `provider`는 manifest entry와 일치해야 한다.

## Tool Metadata Payload Object

tool metadata payload는 하나의 `tool`과 하나의 `provider`에 대한 normalized release index다.

필수 필드:

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `schema_version` | integer | yes | payload schema version. v1은 `1`만 허용한다. |
| `tool` | string | yes | manifest entry의 `tool`과 같아야 한다. |
| `provider` | string | yes | manifest entry의 `provider`와 같아야 한다. |
| `generated_at` | string | yes | RFC3339 UTC 생성 시각. |
| `source` | object | yes | upstream metadata 출처와 생성 도구 정보. |
| `releases` | array | yes | normalized release 목록. |

권장 필드:

| Field | Type | Description |
| --- | --- | --- |
| `min_devenv_version` | string | 특정 payload만 더 높은 CLI version을 요구할 때 사용한다. |
| `metadata` | object | generator version, source checksum, notes 같은 부가 정보. |

### Source Object

필수 필드:

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `kind` | string | yes | `official-api`, `official-index`, `generated-fixture`, `manual-review` 중 하나. |
| `urls` | array | yes | metadata 생성에 사용한 upstream URL 목록. |

권장 필드:

| Field | Type | Description |
| --- | --- | --- |
| `retrieved_at` | string | upstream payload를 가져온 RFC3339 UTC 시각. |
| `generator` | string | catalog payload 생성 도구 이름과 version. |

## Release Object

필수 필드:

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `version` | string | yes | 사용자가 설치 대상으로 볼 exact version. |
| `normalized_version` | string | yes | matching과 sorting에 사용할 normalized version. |
| `aliases` | array | yes | major/minor, LTS, latest 등 matching alias. 없으면 빈 배열. |
| `release_date` | string or null | yes | 알 수 있으면 RFC3339 UTC 또는 `YYYY-MM-DD`, 알 수 없으면 `null`. |
| `selectors` | object | yes | distribution/channel/implementation/stability 같은 release selector. |
| `yanked` | boolean | yes | 기본 install 후보에서 제외할 release이면 `true`. |
| `deprecated` | boolean | yes | 경고 대상이지만 반드시 차단하지는 않는 release이면 `true`. |
| `reason` | string or null | yes | yanked/deprecated 이유. 없으면 `null`. |
| `notes_url` | string or null | yes | release note URL. 없으면 `null`. |
| `artifacts` | array | yes | platform별 artifact 목록. |

정책:

- `yanked=true`인 release는 기본 `install` 후보와 `latest`/prefix matching에서 제외한다.
- 사용자가 exact version을 명시한 yanked release를 설치할지는 별도 unsafe flag 또는 user policy가 결정한다.
- 이미 설치된 runtime 선택은 `yanked=true`만으로 막지 않는다.
- `deprecated=true`는 기본적으로 warning이며, `installable=false`를 자동 의미하지 않는다.
- yanked 또는 deprecated release는 `reason`을 제공해야 한다.

## Artifact Object

필수 필드:

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `platform` | object | yes | DevEnv `Platform`으로 매핑되는 OS/architecture. |
| `url` | string | yes | runtime artifact 다운로드 URL. |
| `filename` | string | yes | 다운로드 cache와 extract policy에 사용할 파일명. |
| `archive_type` | string | yes | `tar.gz`, `tar.xz`, `zip`, `plain-file` 중 하나. |
| `checksum` | string or null | yes | `sha256:<64 lowercase hex>` 또는 checksum이 없으면 `null`. |
| `checksum_algorithm` | string or null | yes | 현재는 `sha256` 또는 `null`. |
| `installable` | boolean | yes | DevEnv가 직접 설치 후보로 사용할 수 있으면 `true`. |
| `install_block_reason` | string or null | yes | `installable=false`인 이유. |
| `metadata` | object | yes | provider-specific 원본 필드 보존 영역. 없으면 빈 object. |

### Platform Object

필수 필드:

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `os` | string | yes | `macos`, `linux`, `windows` 중 하나. |
| `arch` | string | yes | `x64`, `arm64` 중 하나. |

`os`와 `arch`는 core domain의 `OperatingSystem::as_str()`와 `Architecture::as_str()` 결과에 맞춘다.

## Checksum Policy

v1의 checksum 표기는 `sha256:<64 lowercase hex>`로 고정한다.

규칙:

- installable artifact는 `checksum_algorithm="sha256"`와 `checksum="sha256:<hex>"`를 가져야 한다.
- checksum이 없거나 sha256이 아니면 `installable=false`로 표시한다.
- checksum이 없는 artifact를 payload에 남기는 것은 가능하지만, `install_block_reason`에 `missing checksum`처럼 구체적인 사유를 기록해야 한다.
- catalog manifest의 `entries[].sha256`도 같은 `sha256:<hex>` notation을 사용한다.
- artifact checksum 검증은 runtime artifact 다운로드 이후 수행한다.
- manifest payload checksum 검증은 metadata cache write 이전에 수행한다.

## Expiration And Version Policy

`expires_at`:

- 현재 시각보다 미래이면 catalog는 fresh 후보가 될 수 있다.
- 현재 시각보다 과거이면 catalog source는 fresh로 사용하지 않는다.
- `--source catalog`에서 expired catalog만 존재하면 명확한 expired catalog error를 반환한다.
- `auto` source에서는 policy가 허용할 때 official provider refresh 또는 stale cache fallback으로 진행할 수 있다.

`min_devenv_version`:

- manifest 수준 값은 catalog 전체 해석 가능성을 뜻한다.
- entry나 payload 수준 값은 특정 provider payload에만 더 높은 parser가 필요할 때 쓴다.
- 현재 CLI version보다 높으면 fallback으로 숨기지 말고 upgrade guidance를 포함한 error를 반환한다.

`sequence`:

- 같은 `catalog_id`와 trust root에서 단조 증가해야 한다.
- 이전에 신뢰한 sequence보다 낮은 manifest는 rollback 후보이며 기본적으로 거부한다.
- 사내 mirror가 과거 catalog로 고정해야 하면 user/global trust policy에서 명시적으로 허용해야 한다.

## Mapping To Core Domain

| Catalog Field | Core Domain Mapping |
| --- | --- |
| payload `tool` | `RemoteReleaseIndex::tool`, `ToolName` |
| payload `provider` | `RemoteReleaseIndex::provider`, `ProviderId` |
| release `version` | `RemoteRelease::version`, `Version` |
| release `normalized_version` | `RemoteRelease` metadata field `normalized_version`, matching/sorting input |
| release `aliases` | `VersionMatcher` alias/prefix 후보 |
| release `selectors` | `RemoteRelease` metadata fields. 예: `channel`, `distribution`, `implementation`, `stable` |
| release `yanked`, `deprecated`, `reason` | install candidate filtering 전 단계의 release metadata fields |
| artifact `platform` | `ResolvedArtifact::platform`, `Platform` |
| artifact `url`, `filename`, `archive_type`, `checksum` | `Artifact` |
| artifact `installable`, `install_block_reason` | artifact resolution 전 필터링 metadata |
| artifact `metadata` | `ResolvedArtifact::metadata_fields` |

현재 core `Artifact`는 `installable`을 직접 들고 있지 않다. 따라서 catalog parser는 `installable=false` artifact를 `ResolvedArtifact`로 만들기 전에 제외하거나, exact diagnostic이 필요할 때 별도 catalog DTO 단계에서 error를 만들어야 한다. core domain으로 넘어간 artifact는 이미 설치 가능한 후보여야 한다.

## Minimal Manifest Example

```json
{
  "schema_version": 1,
  "catalog_id": "dev.devenv.catalog",
  "generated_at": "2026-05-22T00:00:00Z",
  "expires_at": "2026-05-29T00:00:00Z",
  "catalog_version": "2026.05.22.1",
  "min_devenv_version": "0.1.0",
  "sequence": 1,
  "entries": [
    {
      "tool": "go",
      "provider": "official",
      "path": "go/official/releases.json",
      "sha256": "sha256:<64 lowercase hex>",
      "payload_kind": "normalized-release-index",
      "ttl_seconds": 86400,
      "platforms": ["macos-arm64", "linux-x64"]
    }
  ]
}
```

## Minimal Payload Example

```json
{
  "schema_version": 1,
  "tool": "go",
  "provider": "official",
  "generated_at": "2026-05-22T00:00:00Z",
  "source": {
    "kind": "official-api",
    "urls": ["https://go.dev/dl/?mode=json&include=all"],
    "retrieved_at": "2026-05-22T00:00:00Z",
    "generator": "devenv-catalog-fixture/0.1.0"
  },
  "releases": [
    {
      "version": "1.22.5",
      "normalized_version": "1.22.5",
      "aliases": ["1.22"],
      "release_date": "2024-07-02",
      "selectors": {
        "channel": "stable",
        "distribution": "go"
      },
      "yanked": false,
      "deprecated": false,
      "reason": null,
      "notes_url": "https://go.dev/doc/devel/release#go1.22.5",
      "artifacts": [
        {
          "platform": {
            "os": "linux",
            "arch": "x64"
          },
          "url": "https://go.dev/dl/go1.22.5.linux-amd64.tar.gz",
          "filename": "go1.22.5.linux-amd64.tar.gz",
          "archive_type": "tar.gz",
          "checksum": "sha256:<64 lowercase hex>",
          "checksum_algorithm": "sha256",
          "installable": true,
          "install_block_reason": null,
          "metadata": {
            "go_os": "linux",
            "go_arch": "amd64",
            "kind": "archive"
          }
        }
      ]
    }
  ]
}
```

## Policy Examples

Yanked release:

- release has `yanked=true`.
- release has `reason`.
- default install candidate filtering excludes it.
- exact install policy is deferred to CLI flag/user policy.

Deprecated release:

- release has `deprecated=true`.
- release has `reason`.
- install may continue with warning if all artifacts are installable.

Missing checksum artifact:

- artifact has `checksum=null`.
- artifact has `checksum_algorithm=null`.
- artifact has `installable=false`.
- artifact has `install_block_reason="missing checksum"`.

Expired catalog:

- manifest `expires_at` is in the past.
- catalog source does not refresh the metadata cache from that manifest.
- source mode `auto` may continue to official provider fallback.

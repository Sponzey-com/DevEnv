#!/usr/bin/env sh
set -eu

usage() {
    cat <<'USAGE'
usage: scripts/catalog-smoke.sh [--help]

Runs opt-in live checks for the DevEnv metadata catalog path.

Default behavior:
  No network is used. The script exits successfully and explains how to opt in.

Environment:
  DEVENV_CATALOG_SMOKE=1
      Enable live catalog smoke checks.

  DEVENV_CATALOG_SMOKE_BASE_URL=<catalog-root>
      Catalog root to verify and use for metadata refresh. This may be an
      immutable GitHub Release asset URL root, an HTTP mirror, a file URL, or
      a local catalog directory. If unset, DEVENV_CATALOG_BASE_URL is used.

  DEVENV_CATALOG_SMOKE_DOWNLOAD=1
      Additionally run an actual runtime artifact install smoke. This can
      download a large archive and is disabled unless DEVENV_CATALOG_SMOKE=1
      is also set.

  DEVENV_SMOKE_DEVENV_BIN=/path/to/devenv
      Use an already-built DevEnv binary. If unset, the script runs
      `cargo run --quiet --bin devenv --`.

  DEVENV_CATALOG_SMOKE_GO_VERSION=1.22.5
      Go version used by the optional artifact download smoke.

Checks:
  - Catalog manifest and Go payload verification.
  - Go metadata refresh from the catalog source.
  - Offline Go remote listing from the refreshed metadata cache.
  - Optional Go install smoke when DEVENV_CATALOG_SMOKE_DOWNLOAD=1.
USAGE
}

case "${1:-}" in
    --help|-h)
        usage
        exit 0
        ;;
    "")
        ;;
    *)
        usage >&2
        exit 64
        ;;
esac

repo_root="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
cd "$repo_root"

if [ "${DEVENV_CATALOG_SMOKE:-}" != "1" ]; then
    echo "catalog smoke skipped; set DEVENV_CATALOG_SMOKE=1 and DEVENV_CATALOG_SMOKE_BASE_URL=<catalog-root> to enable live catalog checks"
    exit 0
fi

catalog_root="${DEVENV_CATALOG_SMOKE_BASE_URL:-${DEVENV_CATALOG_BASE_URL:-}}"
if [ -z "$catalog_root" ]; then
    echo "catalog smoke failed: missing catalog root" >&2
    echo "next: set DEVENV_CATALOG_SMOKE_BASE_URL=<catalog-root> or DEVENV_CATALOG_BASE_URL=<catalog-root>" >&2
    exit 65
fi

catalog_root_url() {
    input="$1"
    case "$input" in
        file://*|http://*|https://*)
            printf '%s\n' "$input"
            ;;
        /*)
            encoded="$(printf '%s' "$input" | sed 's/%/%25/g; s/ /%20/g')"
            printf 'file://%s\n' "$encoded"
            ;;
        *)
            encoded="$(printf '%s' "$repo_root/$input" | sed 's/%/%25/g; s/ /%20/g')"
            printf 'file://%s\n' "$encoded"
            ;;
    esac
}

catalog_root="$(catalog_root_url "$catalog_root")"

run_devenv() {
    if [ -n "${DEVENV_SMOKE_DEVENV_BIN:-}" ]; then
        "$DEVENV_SMOKE_DEVENV_BIN" "$@"
    else
        cargo run --quiet --bin devenv -- "$@"
    fi
}

tmp_root="$(mktemp -d "${TMPDIR:-/tmp}/devenv-catalog-smoke.XXXXXX")"
cleanup() {
    rm -rf "$tmp_root"
}
trap cleanup EXIT HUP INT TERM

export DEVENV_HOME="$tmp_root/home"
export DEVENV_ENABLE_CATALOG=1
export DEVENV_CATALOG_BASE_URL="$catalog_root"

# Force the smoke through the catalog/cache path instead of caller fixtures.
unset DEVENV_GO_RELEASE_METADATA
unset DEVENV_GO_OFFICIAL_RELEASE_METADATA
unset DEVENV_JAVA_RELEASE_METADATA
unset DEVENV_JAVA_TEMURIN_RELEASE_METADATA
unset DEVENV_NODE_RELEASE_METADATA
unset DEVENV_NODE_OFFICIAL_RELEASE_INDEX
unset DEVENV_NODE_OFFICIAL_SHASUMS_DIR
unset DEVENV_PYTHON_RELEASE_METADATA
unset DEVENV_FLUTTER_RELEASE_METADATA
unset DEVENV_FLUTTER_OFFICIAL_RELEASES_DIR
unset DEVENV_TERRAFORM_RELEASE_METADATA
unset DEVENV_TERRAFORM_OFFICIAL_RELEASE_INDEX
unset DEVENV_TERRAFORM_OFFICIAL_SHA256SUMS_DIR
unset DEVENV_OPENTOFU_RELEASE_METADATA
unset DEVENV_OPENTOFU_OFFICIAL_RELEASES
unset DEVENV_OPENTOFU_OFFICIAL_SHA256SUMS_DIR

echo "catalog smoke: verifying catalog manifest and Go payload from $catalog_root"
run_devenv metadata verify-catalog go --catalog "$catalog_root" --source catalog

echo "catalog smoke: refreshing Go metadata from catalog"
run_devenv metadata update go --source catalog

echo "catalog smoke: reading Go remote versions from offline catalog cache"
run_devenv list-remote go --offline >/dev/null

if [ "${DEVENV_CATALOG_SMOKE_DOWNLOAD:-}" = "1" ]; then
    go_version="${DEVENV_CATALOG_SMOKE_GO_VERSION:-1.22.5}"
    echo "catalog smoke: installing Go $go_version with artifact download"
    run_devenv install "go@$go_version"
else
    echo "artifact download smoke skipped; set DEVENV_CATALOG_SMOKE_DOWNLOAD=1 to enable it"
fi

echo "catalog smoke ok"

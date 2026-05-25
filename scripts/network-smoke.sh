#!/usr/bin/env sh
set -eu

usage() {
    cat <<'USAGE'
usage: scripts/network-smoke.sh [--help]

Runs opt-in network smoke checks for DevEnv providers.

Default behavior:
  No network is used. The script exits successfully and explains how to opt in.

Environment:
  DEVENV_NETWORK_SMOKE=1
      Enable live metadata network smoke checks.

  DEVENV_NETWORK_SMOKE_DOWNLOAD=1
      Additionally run an actual runtime artifact install smoke. This can download
      a large archive and is disabled unless DEVENV_NETWORK_SMOKE=1 is also set.

  DEVENV_SMOKE_DEVENV_BIN=/path/to/devenv
      Use an already-built DevEnv binary. If unset, the script runs
      `cargo run --quiet --bin devenv --`.

  DEVENV_NETWORK_SMOKE_GO_VERSION=1.22.5
      Go version used by the optional artifact download smoke.

Checks:
  - Go official metadata refresh with `devenv metadata update go`.
  - Offline Go remote listing from the refreshed metadata cache.
  - Optional Go install smoke when DEVENV_NETWORK_SMOKE_DOWNLOAD=1.
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

if [ "${DEVENV_NETWORK_SMOKE:-}" != "1" ]; then
    echo "network smoke skipped; set DEVENV_NETWORK_SMOKE=1 to enable live provider checks"
    exit 0
fi

run_devenv() {
    if [ -n "${DEVENV_SMOKE_DEVENV_BIN:-}" ]; then
        "$DEVENV_SMOKE_DEVENV_BIN" "$@"
    else
        cargo run --quiet --bin devenv -- "$@"
    fi
}

tmp_root="$(mktemp -d "${TMPDIR:-/tmp}/devenv-network-smoke.XXXXXX")"
export DEVENV_HOME="$tmp_root/home"

# Force the smoke through the official network/cache path instead of caller fixtures.
unset DEVENV_GO_RELEASE_METADATA
unset DEVENV_GO_OFFICIAL_RELEASE_METADATA

echo "network smoke: refreshing Go official metadata"
run_devenv metadata update go

echo "network smoke: reading Go remote versions from offline cache"
run_devenv list-remote go --offline >/dev/null

if [ "${DEVENV_NETWORK_SMOKE_DOWNLOAD:-}" = "1" ]; then
    go_version="${DEVENV_NETWORK_SMOKE_GO_VERSION:-1.22.5}"
    echo "network smoke: installing Go $go_version with artifact download"
    run_devenv install "go@$go_version"
else
    echo "artifact download smoke skipped; set DEVENV_NETWORK_SMOKE_DOWNLOAD=1 to enable it"
fi

echo "network smoke ok"

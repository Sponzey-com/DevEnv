#!/usr/bin/env sh
set -eu

if [ "$#" -ne 1 ]; then
    echo "usage: scripts/release-smoke.sh <path-to-devenv-binary>" >&2
    exit 64
fi

binary="$1"

if [ ! -x "$binary" ]; then
    echo "release smoke failed: binary is not executable: $binary" >&2
    exit 1
fi

version_output="$("$binary" --version)"
case "$version_output" in
    *"devenv "*"target="*"profile="*"git="*) ;;
    *)
        echo "release smoke failed: unexpected --version output: $version_output" >&2
        exit 1
        ;;
esac

tmp_root="$(mktemp -d "${TMPDIR:-/tmp}/devenv-release-smoke.XXXXXX")"
DEVENV_HOME="$tmp_root/home" "$binary" shim rehash >/dev/null
doctor_output="$(DEVENV_HOME="$tmp_root/home" "$binary" doctor)"

case "$doctor_output" in
    *"DevEnv doctor"*"status: ok"*) ;;
    *)
        echo "release smoke failed: doctor did not report ok" >&2
        echo "$doctor_output" >&2
        exit 1
        ;;
esac

echo "$version_output"
echo "release smoke ok"

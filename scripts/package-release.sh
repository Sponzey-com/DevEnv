#!/usr/bin/env sh
set -eu

repo_root="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
cd "$repo_root"

version="${DEVENV_RELEASE_VERSION:-}"
if [ -z "$version" ]; then
    version="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)"
fi

if [ -z "$version" ]; then
    echo "failed to determine release version from Cargo.toml" >&2
    exit 1
fi

target="${DEVENV_RELEASE_TARGET:-}"
host_target="$(rustc -vV | sed -n 's/^host: //p')"
if [ -z "$target" ]; then
    target="$host_target"
fi

if [ -z "$target" ]; then
    echo "failed to determine Rust target triple" >&2
    exit 1
fi

if [ -z "$host_target" ]; then
    echo "failed to determine host Rust target triple" >&2
    exit 1
fi

builder="${DEVENV_CARGO:-cargo}"
if [ "${DEVENV_USE_CROSS:-}" = "1" ]; then
    builder="${DEVENV_CROSS:-cross}"
fi

"$builder" build --release --bin devenv --target "$target"

binary_name="devenv"
case "$target" in
    *windows*) binary_name="devenv.exe" ;;
esac

binary_path="target/$target/release/$binary_name"
if [ ! -x "$binary_path" ]; then
    echo "release binary was not built: $binary_path" >&2
    exit 1
fi

out_dir="${DEVENV_DIST_DIR:-target/dist}"
artifact_name="devenv-$version-$target"
archive="$out_dir/$artifact_name.tar.gz"

mkdir -p "$out_dir/stage" "$out_dir/smoke"

if [ -e "$archive" ]; then
    echo "release artifact already exists: $archive" >&2
    exit 1
fi

stage="$(mktemp -d "$out_dir/stage/$artifact_name.XXXXXX")"
package_root="$stage/$artifact_name"
mkdir -p "$package_root"
cp "$binary_path" "$package_root/$binary_name"
cp docs/user-guide.md "$package_root/USER_GUIDE.md"

tar -czf "$archive" -C "$stage" "$artifact_name"

checksum_file="$archive.sha256"
if command -v sha256sum >/dev/null 2>&1; then
    (cd "$out_dir" && sha256sum "$artifact_name.tar.gz") > "$checksum_file"
else
    (cd "$out_dir" && shasum -a 256 "$artifact_name.tar.gz") > "$checksum_file"
fi

smoke_root="$(mktemp -d "$out_dir/smoke/$artifact_name.XXXXXX")"
tar -xzf "$archive" -C "$smoke_root"
smoke_binary="$smoke_root/$artifact_name/$binary_name"
if [ ! -s "$smoke_binary" ]; then
    echo "release smoke failed: packaged binary is missing or empty: $smoke_binary" >&2
    exit 1
fi

smoke_mode="${DEVENV_RELEASE_SMOKE:-auto}"
case "$smoke_mode" in
    auto)
        if [ "$target" = "$host_target" ]; then
            scripts/release-smoke.sh "$smoke_binary"
        else
            echo "release executable smoke skipped: target=$target host=$host_target"
        fi
        ;;
    1|true|yes)
        scripts/release-smoke.sh "$smoke_binary"
        ;;
    0|false|no)
        echo "release executable smoke skipped by DEVENV_RELEASE_SMOKE=$smoke_mode"
        ;;
    *)
        echo "invalid DEVENV_RELEASE_SMOKE value: $smoke_mode" >&2
        echo "expected auto, 1, true, yes, 0, false, or no" >&2
        exit 64
        ;;
esac

echo "$archive"
echo "$checksum_file"

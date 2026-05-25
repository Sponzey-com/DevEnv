#!/usr/bin/env sh
set -eu

usage() {
    cat <<'USAGE'
usage: scripts/package-npm.sh [--pack] [--out-dir <dir>] [--help]

Generate the npm package for @sponzey/devenv from the Cargo workspace version.

Options:
  --pack
      Run npm pack after generating the package directory.

  --out-dir <dir>
      Write generated files under this directory. Defaults to target/npm.

  --help
      Print this help.
USAGE
}

repo_root="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
cd "$repo_root"

out_dir="${DEVENV_NPM_OUT_DIR:-target/npm}"
pack=0

while [ "$#" -gt 0 ]; do
    case "$1" in
        --pack)
            pack=1
            shift
            ;;
        --out-dir)
            if [ "$#" -lt 2 ]; then
                usage >&2
                exit 64
            fi
            out_dir="$2"
            shift 2
            ;;
        --help|-h)
            usage
            exit 0
            ;;
        *)
            usage >&2
            exit 64
            ;;
    esac
done

version="$(sed -n '/^\[workspace\.package\]$/,/^\[/{s/^version = "\(.*\)"/\1/p;}' Cargo.toml | head -n 1)"
if [ -z "$version" ]; then
    echo "failed to determine workspace package version from Cargo.toml" >&2
    exit 1
fi

source_dir="packaging/npm/@sponzey/devenv"
package_dir="$out_dir/@sponzey/devenv"

rm -rf "$package_dir"
mkdir -p "$package_dir/bin" "$package_dir/scripts"

sed "s/__DEVENV_VERSION__/$version/g" "$source_dir/package.template.json" > "$package_dir/package.json"
cp "$source_dir/bin/devenv.js" "$package_dir/bin/devenv.js"
cp "$source_dir/scripts/install.js" "$package_dir/scripts/install.js"
cp README.md "$package_dir/README.md"
cp LICENSE "$package_dir/LICENSE"
chmod +x "$package_dir/bin/devenv.js" "$package_dir/scripts/install.js"

if [ "$pack" = "1" ]; then
    npm_cache="${DEVENV_NPM_CACHE_DIR:-$out_dir/.npm-cache}"
    mkdir -p "$npm_cache"
    npm_config_cache="$npm_cache" npm_config_update_notifier=false npm pack "$package_dir" --pack-destination "$out_dir"
fi

printf '%s\n' "$package_dir"

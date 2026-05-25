#!/usr/bin/env sh
set -eu

repo_root="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
cd "$repo_root"

package_dir="$(scripts/package-npm.sh)"

node --check "$package_dir/bin/devenv.js"
node --check "$package_dir/scripts/install.js"

expected_version="$(sed -n '/^\[workspace\.package\]$/,/^\[/{s/^version = "\(.*\)"/\1/p;}' Cargo.toml | head -n 1)"
actual_version="$(node -e "process.stdout.write(require('./$package_dir/package.json').version)")"
if [ "$expected_version" != "$actual_version" ]; then
    echo "npm package version mismatch: expected $expected_version, got $actual_version" >&2
    exit 1
fi

DEVENV_NPM_SKIP_DOWNLOAD=1 node "$package_dir/scripts/install.js"
npm_cache="${DEVENV_NPM_CACHE_DIR:-target/npm/.npm-cache}"
mkdir -p "$npm_cache"
npm_config_cache="$npm_cache" npm_config_update_notifier=false npm pack "$package_dir" --pack-destination target/npm >/dev/null

echo "npm package smoke passed: @sponzey/devenv@$actual_version"

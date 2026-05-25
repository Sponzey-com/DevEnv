#!/usr/bin/env sh
set -eu

usage() {
    cat <<'USAGE'
usage: scripts/release-version.sh <new-version> [--no-test] [--no-commit] [--no-tag] [--dry-run] [--help]

Prepare a DevEnv release version from the Cargo workspace version.

Default behavior:
  - require a clean git worktree;
  - update Cargo.toml workspace.package.version;
  - run cargo test, which also refreshes Cargo.lock when needed;
  - commit Cargo.toml and Cargo.lock as "Release <version>";
  - create annotated git tag "v<version>".

Options:
  --no-test
      Skip cargo test. Cargo.lock is still refreshed with cargo check.

  --no-commit
      Update files and run verification, but do not create a commit or tag.

  --no-tag
      Create the release commit, but do not create the git tag.

  --dry-run
      Print the planned version change without modifying files.

Examples:
  scripts/release-version.sh 0.1.1
  scripts/release-version.sh 0.2.0-rc.1 --no-tag
  scripts/release-version.sh 0.1.1 --no-commit
USAGE
}

repo_root="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
cd "$repo_root"

new_version=""
run_tests=1
create_commit=1
create_tag=1
dry_run=0

while [ "$#" -gt 0 ]; do
    case "$1" in
        --help|-h)
            usage
            exit 0
            ;;
        --no-test)
            run_tests=0
            shift
            ;;
        --no-commit)
            create_commit=0
            create_tag=0
            shift
            ;;
        --no-tag)
            create_tag=0
            shift
            ;;
        --dry-run)
            dry_run=1
            create_commit=0
            create_tag=0
            shift
            ;;
        -*)
            usage >&2
            exit 64
            ;;
        *)
            if [ -n "$new_version" ]; then
                usage >&2
                exit 64
            fi
            new_version="$1"
            shift
            ;;
    esac
done

if [ -z "$new_version" ]; then
    usage >&2
    exit 64
fi

if ! printf '%s\n' "$new_version" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z][0-9A-Za-z.-]*)?$'; then
    echo "invalid release version: $new_version" >&2
    echo "expected SemVer without a leading v, for example 0.1.1 or 0.2.0-rc.1" >&2
    exit 64
fi

current_version="$(sed -n '/^\[workspace\.package\]$/,/^\[/{s/^version = "\(.*\)"/\1/p;}' Cargo.toml | head -n 1)"
if [ -z "$current_version" ]; then
    echo "failed to determine workspace package version from Cargo.toml" >&2
    exit 1
fi

if [ "$current_version" = "$new_version" ]; then
    echo "release version is already $new_version" >&2
    exit 1
fi

tag_name="v$new_version"
if git rev-parse -q --verify "refs/tags/$tag_name" >/dev/null; then
    echo "release tag already exists: $tag_name" >&2
    exit 1
fi

if [ "$dry_run" = "1" ]; then
    echo "release version dry run"
    echo "current_version $current_version"
    echo "new_version $new_version"
    echo "tag $tag_name"
    exit 0
fi

if [ -n "$(git status --porcelain)" ]; then
    echo "release version requires a clean git worktree" >&2
    echo "commit or stash existing changes before running this script" >&2
    exit 1
fi

tmp_file="$(mktemp "${TMPDIR:-/tmp}/devenv-cargo-toml.XXXXXX")"
awk -v new_version="$new_version" '
    BEGIN {
        in_workspace_package = 0
        changed = 0
    }
    /^\[workspace\.package\]$/ {
        in_workspace_package = 1
        print
        next
    }
    /^\[/ && $0 !~ /^\[workspace\.package\]$/ {
        in_workspace_package = 0
    }
    in_workspace_package == 1 && /^version = / && changed == 0 {
        print "version = \"" new_version "\""
        changed = 1
        next
    }
    {
        print
    }
    END {
        if (changed != 1) {
            exit 42
        }
    }
' Cargo.toml > "$tmp_file" || {
    rm -f "$tmp_file"
    echo "failed to update Cargo.toml workspace package version" >&2
    exit 1
}
mv "$tmp_file" Cargo.toml

echo "updated Cargo.toml workspace version: $current_version -> $new_version"

if [ "$run_tests" = "1" ]; then
    cargo test
else
    cargo check --workspace
fi

if [ "$create_commit" = "1" ]; then
    git add Cargo.toml Cargo.lock
    git commit -m "Release $new_version"
fi

if [ "$create_tag" = "1" ]; then
    git tag -a "$tag_name" -m "Release $new_version"
fi

echo "release version prepared: $new_version"
if [ "$create_tag" = "1" ]; then
    echo "tag created: $tag_name"
else
    echo "tag not created"
fi

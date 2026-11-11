#!/usr/bin/env bash
# Create a release commit and tag from Conventional Commits.
# Usage: mise run release [version]   (version defaults to git-cliff's bumped version)
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

if [ -n "$(git status --porcelain --untracked-files=no)" ]; then
    echo "error: working tree has uncommitted changes" >&2
    exit 1
fi

current="$(cargo pkgid | sed 's/.*[#@]//')"
version="${1:-$(git cliff --bumped-version 2>/dev/null)}"
version="${version#v}"
if [ -z "$version" ] || [ "$version" = "$current" ]; then
    echo "error: no releasable changes since v$current" >&2
    exit 1
fi
tag="v$version"
if git rev-parse -q --verify "refs/tags/$tag" >/dev/null; then
    echo "error: tag $tag already exists" >&2
    exit 1
fi

echo "Releasing $tag (current v$current)"

cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
# Update the [package] version in Cargo.toml; cargo check then syncs Cargo.lock.
VERSION="$version" awk '
    /^\[/ { in_package = ($0 == "[package]") }
    in_package && !done && /^version[[:space:]]*=/ { print "version = \"" ENVIRON["VERSION"] "\""; done = 1; next }
    { print }
' Cargo.toml >Cargo.toml.tmp
mv Cargo.toml.tmp Cargo.toml
cargo check --release
if [ "$(cargo pkgid | sed 's/.*[#@]//')" != "$version" ]; then
    echo "error: failed to set Cargo.toml version to $version" >&2
    exit 1
fi

section="$(git cliff --unreleased --tag "$tag" --strip all)"
if [ -z "$section" ]; then
    echo "error: git-cliff produced an empty changelog section" >&2
    exit 1
fi

# Insert the new section above the first existing version heading.
SECTION="$section" awk '
    !done && /^## \[/ && !/^## \[Unreleased\]/ { print ENVIRON["SECTION"]; print ""; done = 1 }
    { print }
    END { if (!done) { print ""; print ENVIRON["SECTION"] } }
' CHANGELOG.md >CHANGELOG.md.tmp
mv CHANGELOG.md.tmp CHANGELOG.md

git add Cargo.toml Cargo.lock CHANGELOG.md
git commit -m "chore(version): $tag"
git tag -a "$tag" -m "$tag"

echo "Created $tag. Push with: git push --follow-tags"

#!/usr/bin/env bash
# UP4: a release tag must be v + the program's package version, and the
# version a plain MAJOR.MINOR.PATCH. The release workflow runs this before it
# builds anything; run it yourself before tagging:
#   tools/release-check.sh v0.2.0              a release from tag v0.2.0
#   tools/release-check.sh --dry-run           a dry run: just the package version
#   --cargo FILE   the Cargo.toml to read (default app/Cargo.toml)
#   --main REF     also refuse a commit that isn't on REF (e.g. origin/main)
# Prints the version and exits 0, or says why not and exits 1.
set -uo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
cargo="$root/app/Cargo.toml"
main_ref="" tag="" dry=false
usage="usage: tools/release-check.sh [--cargo FILE] [--main REF] (TAG | --dry-run)"
while [ $# -gt 0 ]; do
  case "$1" in
    --cargo) cargo=${2:?$usage}; shift 2 ;;
    --main) main_ref=${2:?$usage}; shift 2 ;;
    --dry-run) dry=true; shift ;;
    -*) echo "$usage" >&2; exit 1 ;;
    *) tag=$1; shift ;;
  esac
done
refuse() { echo "release-check: $*" >&2; exit 1; }
shown=${cargo#"$root"/}   # app/Cargo.toml, as the repo names it
{ $dry || [ -n "$tag" ]; } || refuse "$usage"

# The version line of the [package] table only: dependencies have versions too.
version=$(awk '
  /^[[:space:]]*\[/ { in_pkg = ($0 ~ /^[[:space:]]*\[package\][[:space:]]*$/); next }
  in_pkg && /^[[:space:]]*version[[:space:]]*=/ {
    sub(/^[^=]*=[[:space:]]*"/, ""); sub(/".*$/, ""); print; exit
  }' "$cargo" 2>/dev/null)
[ -n "$version" ] || refuse "$shown has no package version."

plain='^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$'
[[ $version =~ $plain ]] ||
  refuse "the package version $version in $shown isn't a release version: releases are MAJOR.MINOR.PATCH, e.g. 0.2.0."

if ! $dry; then
  [[ $tag == v* ]] || refuse "tag $tag isn't a release tag: release tags start with v, e.g. v$version."
  [[ $tag =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]] ||
    refuse "tag $tag isn't a release tag: release tags are vMAJOR.MINOR.PATCH, e.g. v$version."
  [ "$tag" = "v$version" ] ||
    refuse "tag $tag doesn't match the program's version $version ($shown). Bump the package version to ${tag#v} in the commit you tag, or tag v$version."
fi

if [ -n "$main_ref" ]; then
  git merge-base --is-ancestor HEAD "$main_ref" 2>/dev/null ||
    refuse "commit $(git rev-parse --short HEAD 2>/dev/null) isn't on main ($main_ref): releases are tagged on main."
fi
echo "$version"

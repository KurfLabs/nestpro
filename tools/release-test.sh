#!/usr/bin/env bash
# Tests for the release scripts, in throwaway repos:
#   tools/release-check.sh  the tag must be v + the package version (UP4)
#   tools/release-notes.sh  the commits since the last release, and which of
#                           them change machine programs (UP1)
set -uo pipefail
here=$(cd "$(dirname "$0")/.." && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
fails=0

# Away from this PC's own git settings (a global core.autocrlf, say).
export GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL=/dev/null
git_() { git -c user.name=test -c user.email=test@example.com -c core.autocrlf=false "$@"; }

# expect CODE WORDS DESCRIPTION CMD...: run CMD, want exit CODE and WORDS in what it says.
expect() {
  local want=$1 words=$2 what=$3 out code; shift 3
  out=$("$@" 2>&1); code=$?
  if [ "$code" != "$want" ] || ! grep -qF -- "$words" <<<"$out"; then
    echo "release-test: FAIL $what: wanted exit $want and \"$words\", got exit $code: $out"
    fails=$((fails + 1))
  fi
}
# lacks WORDS DESCRIPTION CMD...: what CMD says must not contain WORDS.
lacks() {
  local words=$1 what=$2 out; shift 2
  out=$("$@" 2>&1)
  if grep -qF -- "$words" <<<"$out"; then
    echo "release-test: FAIL $what: \"$words\" should not be there: $out"
    fails=$((fails + 1))
  fi
}

# ---- release-check.sh: the tag and the package version ----------------------

# cargo VERSION: a Cargo.toml whose package version is VERSION. A dependency's
# version comes first in the file's text after [package] ends, so only the
# [package] table may be read.
cargo() {
  mkdir -p "$work/c"
  cat > "$work/c/Cargo.toml" <<EOF
[package]
name = "demo"
version = "$1"
edition = "2024"

[dependencies]
serde = { version = "9.9.9" }
[dependencies.other]
version = "8.8.8"
EOF
}
check() { "$here/tools/release-check.sh" --cargo "$work/c/Cargo.toml" "$@"; }

cargo 0.2.0
expect 0 "0.2.0" "tag v0.2.0 on package 0.2.0" check v0.2.0
expect 1 "release tags start with v" "tag 0.2.0 without the v" check 0.2.0
expect 1 "vMAJOR.MINOR.PATCH" "tag v0.2 with two parts" check v0.2
expect 1 "doesn't match the program's version 0.2.0" "tag v0.2.1 on package 0.2.0" check v0.2.1
expect 1 "vMAJOR.MINOR.PATCH" "tag v0.2.0-rc1 on package 0.2.0" check v0.2.0-rc1
expect 1 "Bump the package version to 0.2.1" "the refusal says what to do" check v0.2.1
expect 1 "usage" "no tag" check
expect 0 "0.2.0" "a dry run on package 0.2.0" check --dry-run
cargo 0.2.0-rc1
expect 1 "isn't a release version" "package 0.2.0-rc1 tagged v0.2.0-rc1" check v0.2.0-rc1
expect 1 "isn't a release version" "a dry run on package 0.2.0-rc1" check --dry-run
cargo 0.2
expect 1 "isn't a release version" "package 0.2" check --dry-run
cargo 1.10.0
expect 0 "1.10.0" "two-digit parts" check v1.10.0
expect 1 "doesn't match" "1.1.0 is not 1.10.0" check v1.1.0
printf '[package]\nname = "demo"\n' > "$work/c/Cargo.toml"
expect 1 "no package version" "a Cargo.toml without a version" check v0.2.0

# The tagged commit must be on main: a tag on a side branch is refused.
rm -rf "$work/m" && mkdir -p "$work/m" && cd "$work/m" || exit 1
git_ init -q -b main . && echo a > a && git_ add a && git_ commit -qm base
git_ switch -qc side && echo b > b && git_ add b && git_ commit -qm side
cargo 0.2.0
expect 1 "isn't on main" "a tag on a side branch" check --main main v0.2.0
git_ switch -q main
expect 0 "0.2.0" "a tag on main" check --main main v0.2.0

# ---- release-notes.sh: the commits and the machine-program section ----------

# A repo whose AGENTS.md names the risky paths the way this repo's does; it
# changes machine programs once in a merged branch and once directly.
rm -rf "$work/r" && mkdir -p "$work/r/tools" "$work/r/app/src" "$work/r/docs" && cd "$work/r" || exit 1
git_ init -q -b main .
cat > AGENTS.md <<'EOF'
# Agent guide

- **Check:** `tools/check.sh`.
- **Risky paths:** anything that changes a machine program (`app/src/cnc.rs`, `app/src/post.rs`,
  `app/src/kerf.rs`), and the public
  check (`tools/public-check.sh`). The final PR stops for the user.
- **Hotspot files:** `FEATURES.md`.
EOF
for f in app/src/cnc.rs app/src/post.rs app/src/kerf.rs app/src/ui.rs tools/public-check.sh docs/a.md; do echo 1 > "$f"; done
git_ add -A && git_ commit -qm "First release" && git_ tag v0.1.0
echo 2 > docs/a.md && git_ commit -qam "Explain the stock list"
docs_only=$(git rev-parse HEAD)
git_ switch -qc rapids
echo 2 > app/src/cnc.rs && git_ commit -qam "Shorter rapids, part one"
echo 2 > app/src/post.rs && echo 2 > app/src/ui.rs && git_ commit -qam "Shorter rapids, part two"
git_ switch -q main && git_ merge -q --no-ff rapids -m "Merge PR #5: Shorter rapids between parts"
echo 3 > app/src/ui.rs && git_ commit -qam "Bigger buttons"
# A merge made on GitHub's site: the subject names the branch, the body the title.
git_ switch -qc acme/joints && echo 2 > app/src/kerf.rs && git_ commit -qam "wip"
git_ switch -q main && git_ merge -q --no-ff acme/joints -m "Merge pull request #7 from acme/joints" -m "Joints on small holes"
echo 2 > tools/public-check.sh && git_ commit -qam "Stricter public check"
echo 3 > app/src/kerf.rs && git_ commit -qam "Kerf on small holes"
notes() { "$here/tools/release-notes.sh" --agents "$work/r/AGENTS.md" "$@"; }

expect 0 "## Changes machine programs" "the section is there" notes
expect 0 "- Shorter rapids between parts (#5): app/src/cnc.rs, app/src/post.rs" "a merge touching two risky files, named once with both" notes
expect 0 "- Kerf on small holes (" "a direct commit touching a risky file" notes
expect 0 "Since v0.1.0: 6 changes." "the range starts at the last release tag" notes
expect 0 "- Joints on small holes (#7): app/src/kerf.rs" "a merge made on GitHub's site, by its title" notes
lacks "from acme/joints" "not by its branch" notes
expect 0 "- Explain the stock list (" "every change is listed" notes
expect 0 "- Stricter public check (" "the public check is listed as a change" notes
lacks "Shorter rapids, part one" "a merged branch's own commits are folded into its merge" notes
lacks "First release" "the release before isn't in the notes" notes
# Only the machine section names files, so a file name under it is enough.
lacks "tools/public-check.sh" "the public check is not a machine program" notes
lacks "Bigger buttons (" "a change to no risky file is only under Changes" sh -c "\"$here/tools/release-notes.sh\" --agents \"$work/r/AGENTS.md\" | sed '/^## Changes\$/,\$d'"

expect 0 "None: no change in this release touches the code that writes machine programs." "a range with no machine-program changes" notes v0.1.0 "$docs_only"
expect 0 "- Explain the stock list (" "an explicit range" notes v0.1.0 "$docs_only"

# Tagging the new release: its notes run from the previous tag, not from itself.
git_ tag v0.2.0
expect 0 "Since v0.1.0: 6 changes." "notes ending at the tag" notes "" v0.2.0

# No release before: everything from the first commit.
git_ tag -d v0.1.0 >/dev/null
expect 0 "First release" "no earlier tag: the whole history" notes "" v0.2.0
expect 0 "Since the first commit: 7 changes." "no earlier tag: counted from the start" notes "" v0.2.0

# AGENTS.md without the list: refuse, never say "None".
printf '# Agent guide\n' > "$work/no-list.md"
expect 1 "no machine-program paths" "an AGENTS.md without the risky paths" "$here/tools/release-notes.sh" --agents "$work/no-list.md"
expect 1 "not a commit" "a range end that doesn't exist" notes v0.2.0 nosuchref

# This repo's own AGENTS.md has the list.
cd "$here" || exit 1
expect 0 "## Changes machine programs" "this repo's AGENTS.md" tools/release-notes.sh HEAD~1 HEAD

if [ "$fails" -gt 0 ]; then echo "release-test: $fails failed"; exit 1; fi
echo "release-test: ok"

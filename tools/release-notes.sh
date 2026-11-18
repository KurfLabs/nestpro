#!/usr/bin/env bash
# UP1: a release's notes, in Markdown: every change since the previous release,
# and first a section "Changes machine programs" naming the changes that touch
# the code that writes machine programs, so the shop test-cuts before rolling
# the release out. That code is the list of risky paths in AGENTS.md ("anything
# that changes a machine program (...)"), read from there each time.
#   tools/release-notes.sh                  from the last release tag on main to HEAD
#   tools/release-notes.sh FROM [TO]        between two commits
#   tools/release-notes.sh "" v0.2.0        from the tag before v0.2.0 to v0.2.0
#   --agents FILE   the AGENTS.md to read (default the repo's own)
# A change is a commit on main's own line (the first parent), so a merged pull
# request counts once, with everything it changed. A moved file counts under
# its old path and its new one.
set -uo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
agents="$root/AGENTS.md"
args=()
while [ $# -gt 0 ]; do
  case "$1" in
    --agents) agents=${2:?--agents needs a file}; shift 2 ;;
    *) args+=("$1"); shift ;;
  esac
done
from=${args[0]:-} to=${args[1]:-HEAD}
refuse() { echo "release-notes: $*" >&2; exit 1; }

# The paths in the parentheses after "machine program" in the Risky paths item,
# which may run over several lines until the next item.
risky=$(awk '
  /^- \*\*Risky paths:\*\*/ { on = 1 }
  on && /^- \*\*/ && !/Risky paths/ { exit }
  on { printf "%s ", $0 }' "$agents" 2>/dev/null |
  sed -n 's/.*machine program[s]* (\([^)]*\)).*/\1/p' | grep -o '`[^`]*`' | tr -d '`')
[ -n "$risky" ] || refuse "found no machine-program paths in $agents: its \"Risky paths\" item must list them as \"anything that changes a machine program (\`app/src/…\`, …)\"."

git rev-parse -q --verify "$to^{commit}" >/dev/null || refuse "$to is not a commit."
if [ -z "$from" ]; then
  # The last release: a plain vX.Y.Z tag on main's own line, never a
  # pre-release (v0.2.0-rc1) or a tag on a merged branch's commit.
  # Only tags the release check accepts (vMAJOR.MINOR.PATCH, nothing after):
  # v1.2.2+build.1 or v0.2.0-rc1 are never a release.
  # A lightweight tag has no peeled object: its first field is the commit.
  tags=$(git for-each-ref refs/tags --format='%(*objectname) %(objectname) %(refname:short)' 2>/dev/null |
    awk '$NF ~ /^v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$/ { print $1, $NF }') || tags=""
  # awk reads the whole list (no early exit), so git never writes into a closed pipe.
  from=$(git rev-list --first-parent "$to^" 2>/dev/null |
    awk -v tags="$tags" '
      BEGIN { n = split(tags, line, "\n"); for (i = 1; i <= n; i++) { split(line[i], f, " "); tag[f[1]] = f[2] } }
      found == "" && ($1 in tag) { found = tag[$1] }
      END { if (found != "") print found }') || from=""
fi
if [ -n "$from" ]; then
  git rev-parse -q --verify "$from^{commit}" >/dev/null || refuse "$from is not a commit."
  range="$from..$to" since="Since $from"
else
  range="$to" since="Since the first commit"
fi

# One record per change: \x01 hash \t subject, then the files it changed
# (a merge: what it changed on main's line).
changes=() machine=()
while IFS= read -r -d $'\x01' rec; do
  [ -n "$rec" ] || continue
  head=${rec%%$'\n'*}
  hash=${head%%$'\t'*} subject=${head#*$'\t'}
  # "Merge PR #5: Shorter rapids" reads "Shorter rapids (#5)".
  if [[ $subject =~ ^Merge\ PR\ \#([0-9]+):\ (.*)$ ]]; then
    line="${BASH_REMATCH[2]} (#${BASH_REMATCH[1]})"
  elif [[ $subject =~ ^Merge\ pull\ request\ \#([0-9]+)\ from\  ]]; then
    # Merged on GitHub's site: the subject names the branch, the body's first
    # line is the pull request's title.
    num=${BASH_REMATCH[1]}
    title=$(git log -1 --format=%b "$hash" | sed -n '/[^[:space:]]/{p;q;}')
    line="${title:-$subject} (#$num)"
  else
    line="$subject ($hash)"
  fi
  changes+=("- $line")
  hits=()
  while IFS= read -r f; do
    [ -n "$f" ] || continue
    while IFS= read -r p; do
      if [ "$f" = "$p" ] || { [[ $p == */ ]] && [[ $f == "$p"* ]]; }; then hits+=("$f"); break; fi
    done <<<"$risky"
  done < <(printf '%s\n' "${rec#*$'\n'}" | sort -u)
  if [ ${#hits[@]} -gt 0 ]; then
    joined=$(printf '%s, ' "${hits[@]}"); machine+=("- $line: ${joined%, }")
  fi
done < <(git log --first-parent --diff-merges=first-parent --name-only --no-renames \
           --format=$'\x01%h\t%s' "$range" && printf '\x01')

n=${#changes[@]}
[ "$n" = 1 ] && word=change || word=changes
echo "$since: $n $word."
echo
echo "## Changes machine programs"
echo
if [ ${#machine[@]} -gt 0 ]; then
  echo "Test-cut before rolling this release out: these change the code that writes machine programs."
  echo
  printf '%s\n' "${machine[@]}"
else
  echo "None: no change in this release touches the code that writes machine programs."
fi
echo
echo "## Changes"
echo
[ "$n" -gt 0 ] && printf '%s\n' "${changes[@]}"
exit 0

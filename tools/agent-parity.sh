#!/usr/bin/env bash
# app/src/engine/gui_unreviewed.txt lists the panel buttons, menu entries and
# tick boxes that were in the app before the agent-parity check covered them
# (AGT-8). It may only get shorter: a new control gets a line in
# engine::ops::GUI_MAP instead (the test every_panel_control_is_mapped says
# which). This fails when a branch adds lines to the list.
#   tools/agent-parity.sh [BASE]   BASE defaults to where HEAD left origin/main (or main)
set -uo pipefail
cd "$(dirname "$0")/.."
list=app/src/engine/gui_unreviewed.txt

base=${1:-}
if [ -z "$base" ]; then
  for b in origin/main main; do
    if git rev-parse -q --verify "$b^{commit}" >/dev/null; then base=$(git merge-base HEAD "$b") && break; fi
  done
fi
if [ -z "$base" ]; then echo "agent-parity: no main branch to compare with; skipped"; exit 0; fi
if ! git cat-file -e "$base:$list" 2>/dev/null; then echo "agent-parity: $list is new on this branch; ok"; exit 0; fi

added=$(git diff "$base" -- "$list" | tr -d '\r' | grep -E '^\+[^+]' | grep -vE '^\+[[:space:]]*(#|$)')
if [ -n "$added" ]; then
  echo "agent-parity: lines added to $list (it only gets shorter):"
  echo "$added" | sed 's/^+/  /'
  echo "Map each in engine::ops::GUI_MAP instead: the operation that does the same for an agent, or view:/gui:/todo: [ID] with a reason."
  exit 1
fi
echo "agent-parity: ok"

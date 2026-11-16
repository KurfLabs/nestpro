#!/usr/bin/env bash
# Run the Linux tests and the Windows type-check on a boat.dev cloud machine
# instead of this computer, so several checkouts can test at once without
# fighting over its cores.
#
#   tools/boat-check.sh              test this checkout on a fresh machine
#   tools/boat-check.sh --make-base  save the base image the runs start from
#   tools/check.sh --boat            the whole check, with these two steps here
#
# What is sent: the files the public check covers (tracked and untracked,
# never git-ignored ones, so no vendor/, research/ or target/). Each run gets
# its own machine, which is deleted when the run ends, even on Ctrl-C.
#
# The base image ("nestpro-ci") has the system libraries, the Windows target
# and every library crate prebuilt, so a run only rebuilds the app. Remake it
# after Cargo.lock changes much; an old one is only slower, never wrong,
# because the sent files get fresh timestamps and the app is always rebuilt.
#
# Needs the boat CLI, signed in (https://boat.dev/install, then boat login).
# BOAT_TYPE picks the machine size: default (4 CPUs), large (8 CPUs, 2x rate).
set -uo pipefail
cd "$(dirname "$0")/.."

BASE=nestpro-ci
TYPE=${BOAT_TYPE:-default}
BOAT=${BOAT:-$(command -v boat || echo "$HOME/.ascii/bin/boat")}
export BOAT_NO_UPDATE=1
[ -x "$BOAT" ] || { echo "== boat CLI not found. Install it (https://boat.dev/install) and sign in with boat login, or run tools/check.sh here."; exit 1; }

# System libraries the Linux build needs (the same as the CI test-linux job).
SETUP='sudo apt-get update -qq >/dev/null &&
  sudo DEBIAN_FRONTEND=noninteractive apt-get install -y -qq libgtk-3-dev libxkbcommon-dev libgl1-mesa-dev >/dev/null 2>&1 &&
  rustup target add x86_64-pc-windows-gnu >/dev/null 2>&1'

# Run a script on the machine, print its output as it comes, return its exit code.
run() {
  "$BOAT" exec "$1" --timeout 0 --json -- bash -lc "$2" </dev/null | python3 -I -c '
import json, sys
code = 1  # a stream that ends without an exit frame counts as a failure
for line in sys.stdin:
    try:
        f = json.loads(line)
    except ValueError:
        sys.stderr.write(line); continue
    t = f.get("type")
    if t in ("stdout", "stderr"):
        (sys.stdout if t == "stdout" else sys.stderr).write(f.get("data", ""))
        sys.stdout.flush()
    elif t == "exit":
        code = f["exitCode"] if f.get("exitCode") is not None else 1
sys.exit(code)'
}

box=""
cleanup() {
  [ -n "$box" ] && "$BOAT" delete "$box" --yes --json </dev/null >/dev/null 2>&1 && echo "== Deleted boat machine $box"
  rm -f "${tgz:-}"
}
trap cleanup EXIT
trap 'exit 130' INT TERM

start() { # start <name> [--from BASE]
  local out
  out=$("$BOAT" new --type "$TYPE" --ttl 5400 --name "$1" --json "${@:2}" </dev/null 2>&1)
  box=$(printf '%s\n' "$out" | jq -r 'select(.event? == "ready") | .id' 2>/dev/null | tail -1)
  [ -n "$box" ] || { printf '%s\n' "$out" | tail -5; return 1; }
}

# Nothing leaves this computer unless the public check passes on the same files.
tools/public-check.sh >/dev/null 2>&1 || { echo "== Nothing sent: the public check fails here (run tools/public-check.sh)"; exit 1; }

# Pack what the public check covers; files deleted but not yet committed are skipped.
tgz=$(mktemp --suffix=.tgz)
git ls-files -z --cached --others --exclude-standard \
  | while IFS= read -r -d '' f; do [ -e "$f" ] && printf '%s\0' "$f"; done \
  | tar --null -T - -czf "$tgz" || { echo "== Could not pack the files to send"; exit 1; }

# Replace the old copy but keep app/target; -m gives every file a fresh time
# so cargo never trusts a build of other code.
SYNC='set -e; mkdir -p nestpro && cd nestpro
  find . -mindepth 1 -maxdepth 1 ! -name app -exec rm -rf {} +
  if [ -d app ]; then find app -mindepth 1 -maxdepth 1 ! -name target -exec rm -rf {} +; fi
  tar xzmf ~/src.tgz && rm ~/src.tgz'

send() {
  "$BOAT" scp "$tgz" "$box:src.tgz" </dev/null >/dev/null 2>&1 \
    && run "$box" "$SYNC" >/dev/null || { echo "== Could not copy the files to boat machine $box"; return 1; }
}

if [ "${1:-}" = "--make-base" ]; then
  echo "== Making the base image $BASE on a $TYPE boat machine"
  start "$BASE-build" || { echo "== FAILED: could not start a boat machine"; exit 1; }
  send || exit 1
  run "$box" "set -o pipefail; $SETUP && cd nestpro/app && cargo test --no-run 2>&1 | tail -1 &&
      cargo check --target x86_64-pc-windows-gnu 2>&1 | tail -1 &&
      sha256sum Cargo.lock > ~/base-lock.sha" \
    || { echo "== FAILED: the base build"; exit 1; }
  "$BOAT" snapshot "$box" "$BASE" --json </dev/null >/dev/null || { echo "== FAILED: could not save $BASE"; exit 1; }
  echo "== Saved the base image $BASE"
  exit 0
fi

name="nestpro-check $(git rev-parse --abbrev-ref HEAD 2>/dev/null)"
if start "$name" --from "$BASE"; then
  echo "== Running on boat machine $box ($TYPE, from $BASE)"
  prep=true
else
  echo "== Could not start from the base image $BASE (above; make it with tools/boat-check.sh --make-base); setting up a bare machine, about 6 minutes slower"
  start "$name" || { echo "== FAILED: could not start a boat machine"; exit 1; }
  echo "== Running on boat machine $box ($TYPE, bare)"
  prep=$SETUP
fi
send || { echo "== FAILED: boat copy"; exit 1; }

run "$box" "$prep || { echo '== Machine setup failed'; exit 1; }
  cd nestpro/app
  [ ! -f ~/base-lock.sha ] || sha256sum --status -c ~/base-lock.sha ||
    echo '== Note: Cargo.lock changed since the base image; changed crates rebuild (tools/boat-check.sh --make-base refreshes it)'
  failed=()
  echo '== Linux tests';   cargo test 2>&1 | grep -E '^test result|FAILED|panicked|^error'
  [ \"\${PIPESTATUS[0]}\" = 0 ] || failed+=('Linux tests')
  echo '== Windows check'; cargo check --target x86_64-pc-windows-gnu 2>&1 | grep -E '^(warning|error)|Finished'
  [ \"\${PIPESTATUS[0]}\" = 0 ] || failed+=('Windows check')
  if [ \${#failed[@]} -gt 0 ]; then echo \"== FAILED on boat: \${failed[*]}\"; exit 1; fi
  echo '== Boat checks passed'"

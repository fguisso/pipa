#!/bin/sh
# agent-login.sh — start a pipa device-flow login and print ONLY the verify
# URL, so an agent can hand it to a human to approve in a browser. The login
# keeps running in the background and finishes once the human approves; confirm
# afterwards with `pipa whoami --json` (look for "logged_in":true).
#
#   ./agent-login.sh <server-url> [device-label]
#
# The <server-url> MUST come from the human — do not guess it.
set -eu

[ $# -ge 1 ] || { echo "usage: agent-login.sh <server-url> [label]" >&2; exit 2; }
SERVER="$1"
LABEL="${2:-agent}"

command -v pipa >/dev/null 2>&1 || { echo "pipa is not installed (run the installer first)" >&2; exit 1; }

OUT="$(mktemp)"
# Detach so the login survives this script and keeps polling for approval.
nohup pipa login --server "$SERVER" --label "$LABEL" --json >"$OUT" 2>&1 &

# device-init is near-instant; wait up to ~15s for the verify_url line.
i=0
while [ "$i" -lt 15 ]; do
  url=$(sed -n 's/.*"verify_url":"\([^"]*\)".*/\1/p' "$OUT" 2>/dev/null | head -n1)
  if [ -n "$url" ]; then
    printf '%s\n' "$url"
    exit 0
  fi
  i=$((i + 1))
  sleep 1
done

echo "timed out waiting for verify_url. login output:" >&2
cat "$OUT" >&2
exit 1

#!/bin/sh
# agent-stepup.sh — run a pipa command that requires a step-up confirmation
# (loosening security: access=noauth or zone=public, or `rm`), printing ONLY
# the step-up URL for a human to approve in a browser. The command keeps
# running in the background and completes once approved.
#
#   ./agent-stepup.sh share <uuid> --zone public
#   ./agent-stepup.sh share <uuid> --access noauth
#   ./agent-stepup.sh rm <uuid>
#
# After the human approves, the operation finishes on its own; verify with
# e.g. `pipa get <uuid> --json`. The raw result is also written to the temp
# file printed on stderr.
set -eu

[ $# -ge 1 ] || { echo "usage: agent-stepup.sh <pipa-args...>" >&2; exit 2; }
command -v pipa >/dev/null 2>&1 || { echo "pipa is not installed (run the installer first)" >&2; exit 1; }

OUT="$(mktemp)"
nohup pipa "$@" --json >"$OUT" 2>&1 &

i=0
while [ "$i" -lt 15 ]; do
  url=$(sed -n 's/.*"verify_url":"\([^"]*\)".*/\1/p' "$OUT" 2>/dev/null | head -n1)
  if [ -n "$url" ]; then
    printf '%s\n' "$url"
    printf 'result will be written to: %s\n' "$OUT" >&2
    exit 0
  fi
  i=$((i + 1))
  sleep 1
done

echo "timed out waiting for step-up url. output:" >&2
cat "$OUT" >&2
exit 1

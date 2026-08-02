#!/usr/bin/env bash
#
# Refuse to ship a committed credential.
#
# SimpleBar makes no network calls and holds no secrets by design — the whole
# point of the statusLine route is that no token is ever involved. So this is a
# guard against accident, not against the architecture:
#
#   * a real token pasted into a test fixture while debugging
#   * somebody's actual ~/.claude/settings.json or .credentials.json committed
#   * usage.json committed, which is account usage state rather than source
#
# It scans tracked files when the repo has commits, and everything not ignored
# otherwise, so it is useful before the very first commit too.
#
# Written for what THIS repo actually holds. Do not copy it into a project with
# a database or a cloud provider and assume it covers them.
#
# Portability: macOS ships bash 3.2, so no mapfile and no associative arrays.
# NUL-delimited lists throughout, because filenames may contain spaces.

set -euo pipefail

cd "$(dirname "$0")/.."

SELF="scripts/check-secrets.sh"
failed=0

list=$(mktemp)
filtered=$(mktemp)
trap 'rm -f "$list" "$filtered"' EXIT

if git rev-parse --verify HEAD >/dev/null 2>&1; then
    git ls-files -z >"$list"
else
    git ls-files -z --cached --others --exclude-standard >"$list"
fi

# Drop this script — it necessarily contains the patterns it looks for — and
# anything that is not a regular file.
count=0
while IFS= read -r -d '' f; do
    [ "$f" = "$SELF" ] && continue
    [ -f "$f" ] || continue
    printf '%s\0' "$f" >>"$filtered"
    count=$((count + 1))
done <"$list"

if [ "$count" -eq 0 ]; then
    echo "check-secrets: nothing to scan."
    exit 0
fi

report() {
    echo "check-secrets: FAIL — $1"
    failed=1
}

# Credential-shaped strings.
if xargs -0 grep -InE \
    -e 'sk-ant-[A-Za-z0-9_-]{16,}' \
    -e 'BEGIN [A-Z ]*PRIVATE KEY' \
    -e 'AKIA[0-9A-Z]{16}' \
    -e '(access|refresh)_token"[[:space:]]*:[[:space:]]*"[A-Za-z0-9._-]{20,}' \
    -e 'Authorization:[[:space:]]*Bearer[[:space:]]+[A-Za-z0-9._-]{20,}' \
    <"$filtered"; then
    report "a credential-shaped string is present in the files above."
fi

# Files that should never be in this repo at all.
while IFS= read -r -d '' f; do
    case "$(basename "$f")" in
    .credentials.json | settings.json | settings.local.json)
        report "$f — that is a Claude Code config file, not project source."
        ;;
    usage.json)
        report "$f — that is account usage state; it belongs in \$XDG_STATE_HOME."
        ;;
    esac
done <"$filtered"

if [ "$failed" -eq 0 ]; then
    echo "check-secrets: clean ($count files scanned)."
fi

exit "$failed"

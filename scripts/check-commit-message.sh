#!/usr/bin/env bash
# Conventional Commits gate — no external dependencies (see AGENTS.md §3).
set -euo pipefail

msg_file="${1:?usage: check-commit-message.sh <commit-msg-file>}"
first_line="$(head -n1 "$msg_file")"

# Git-generated messages (merge commits, reverts) are exempt from the gate.
case "$first_line" in
    "Merge "*|'Revert "'*) exit 0 ;;
esac

pattern='^(feat|fix|docs|chore|ci|test|refactor|perf|build)(\([a-z0-9._-]+\))?!?: .{1,72}$'

if [[ "$first_line" =~ $pattern ]]; then
    exit 0
fi

echo "✗ Commit message does not follow Conventional Commits:" >&2
echo "    $first_line" >&2
echo "  Expected: type(scope)?: subject   (type ∈ feat|fix|docs|chore|ci|test|refactor|perf|build)" >&2
exit 1

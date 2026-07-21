#!/usr/bin/env bash
set -euo pipefail

readonly workflows_dir=.github/workflows
readonly -a scanned_paths=(
  "$workflows_dir"
  Makefile
  compose.yaml
  .github/scripts
)

status=0
while IFS= read -r match; do
  target=${match#*uses:}
  target=${target%%#*}
  target=${target//[[:space:]]/}
  target=${target#\"}
  target=${target%\"}
  target=${target#\'}
  target=${target%\'}

  if [[ "$target" != ./* && ! "$target" =~ @[0-9a-f]{40}$ ]]; then
    echo "floating GitHub Action: $match" >&2
    status=1
  fi
done < <(rg -n '^[[:space:]]*-?[[:space:]]*uses:[[:space:]]*' "$workflows_dir")

# Split these literals in the script source so scanning repository-owned
# scripts does not make the gate report its own detection expressions.
readonly floating_pattern='releases/'latest'|ubuntu-'latest'|npx --'yes
if rg -n "$floating_pattern" "${scanned_paths[@]}"; then
  echo "floating executable input found" >&2
  status=1
fi

exit "$status"

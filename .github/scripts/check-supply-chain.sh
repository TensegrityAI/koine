#!/usr/bin/env bash
set -euo pipefail

readonly script_dir=$(cd -- "${BASH_SOURCE[0]%/*}" && pwd)
readonly repository_root=$(cd -- "$script_dir/../.." && pwd)

if ! command -v node >/dev/null 2>&1; then
  echo "required Node runtime not found" >&2
  exit 2
fi
if [[ ! -f "$repository_root/node_modules/js-yaml/index.js" ]]; then
  echo "required parser module missing; install repository tooling first" >&2
  exit 2
fi

exec node "$script_dir/check-supply-chain.mjs" "$@"

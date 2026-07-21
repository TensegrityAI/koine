#!/usr/bin/env bash
set -euo pipefail

readonly script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
readonly repository_root=$(cd -- "$script_dir/../.." && pwd)
readonly gate="$repository_root/.github/scripts/check-supply-chain.sh"
readonly fixtures="$repository_root/.github/supply-chain-fixtures"
declare -a temporary_roots=()
fixture_path=

cleanup() {
  local root
  for root in "${temporary_roots[@]}"; do
    if [[ -d "$root" ]]; then
      /usr/bin/find "$root" -depth -delete
    fi
  done
}
trap cleanup EXIT

fixture_root() {
  local overlay=${1-}
  local root
  root=$(mktemp -d)
  temporary_roots+=("$root")
  cp -R "$fixtures/base/." "$root/"
  if [[ -n "$overlay" ]]; then
    cp -R "$fixtures/$overlay/." "$root/"
  fi
  fixture_path=$root
}

expect_pass() {
  local name=$1
  local overlay=${2-}
  local root
  fixture_root "$overlay"
  root=$fixture_path
  if ! "$gate" --root "$root" >"$root/output" 2>&1; then
    echo "FAIL $name: expected success" >&2
    sed -n '1,120p' "$root/output" >&2
    return 1
  fi
  echo "PASS $name"
}

expect_fail() {
  local name=$1
  local overlay=$2
  local expected=$3
  local root
  fixture_root "$overlay"
  root=$fixture_path
  if "$gate" --root "$root" >"$root/output" 2>&1; then
    echo "FAIL $name: mutation was accepted" >&2
    return 1
  fi
  if ! rg -Fq -- "$expected" "$root/output"; then
    echo "FAIL $name: missing diagnostic: $expected" >&2
    sed -n '1,120p' "$root/output" >&2
    return 1
  fi
  echo "PASS $name"
}

expect_pass valid
expect_pass comments pass/comments
expect_pass quoted_and_flow_actions pass/action-syntax

expect_fail action_tag fail/action-tag "floating or unapproved GitHub Action"
expect_fail action_without_comment fail/action-no-comment "approved release comment"
expect_fail action_bad_comment fail/action-bad-comment "approved release comment"
expect_fail quoted_action_tag fail/quoted-action-tag "floating or unapproved GitHub Action"
expect_fail flow_action_tag fail/flow-action-tag "floating or unapproved GitHub Action"
expect_fail npx_long_option fail/npx-long-option "floating npm execution"
expect_fail npx_short_option fail/npx-short-option "floating npm execution"
expect_fail tla_execution_without_recheck fail/tla-no-run-check "TLA+ execution must verify"
expect_fail tla_download_without_checksum fail/tla-no-download-check "TLA+ download must verify"
expect_fail npm_install fail/npm-install "npm install is not allowed"
expect_fail package_version_drift fail/package-version "package.json must pin markdownlint-cli2"
expect_fail node_engine_upper_bound fail/node-engine-upper "package.json must constrain Node to >=22.23.1"
expect_fail setup_node_drift fail/setup-node "floating or unapproved GitHub Action"
expect_fail node_version_drift fail/node-version "Node version drift"
expect_fail image_tag_drift fail/image-tag "temporary Postgres image exception drifted"
expect_fail image_without_digest fail/image-no-digest "container image must use a sha256 digest"
expect_fail unapproved_curl fail/unapproved-curl "unapproved executable download"
expect_fail unapproved_wget fail/unapproved-wget "unapproved executable download"

fixture_root
missing_rg_root=$fixture_path
missing_rg_path=$(mktemp -d)
temporary_roots+=("$missing_rg_path")
if PATH="$missing_rg_path" /usr/bin/bash "$gate" --root "$missing_rg_root" \
  >"$missing_rg_root/output" 2>&1; then
  echo "FAIL missing_rg: gate accepted a missing scanner" >&2
  exit 1
fi
if ! /usr/bin/grep -Fq "required scanner 'rg' not found" "$missing_rg_root/output"; then
  echo "FAIL missing_rg: missing diagnostic" >&2
  exit 1
fi
echo "PASS missing_rg"

fixture_root
broken_rg_root=$fixture_path
broken_rg_path=$(mktemp -d)
temporary_roots+=("$broken_rg_path")
printf '#!/usr/bin/bash\necho "simulated read error" >&2\nexit 2\n' \
  >"$broken_rg_path/rg"
chmod +x "$broken_rg_path/rg"
if PATH="$broken_rg_path" /usr/bin/bash "$gate" --root "$broken_rg_root" \
  >"$broken_rg_root/output" 2>&1; then
  echo "FAIL scanner_error: gate accepted a scanner failure" >&2
  exit 1
fi
if ! /usr/bin/grep -Fq "scanner failed" "$broken_rg_root/output"; then
  echo "FAIL scanner_error: missing diagnostic" >&2
  exit 1
fi
echo "PASS scanner_error"

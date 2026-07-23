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
  if ! grep -Fq -- "$expected" "$root/output"; then
    echo "FAIL $name: missing diagnostic: $expected" >&2
    sed -n '1,120p' "$root/output" >&2
    return 1
  fi
  echo "PASS $name"
}

expect_pass valid
expect_pass comments pass/comments
expect_pass quoted_scalars pass/quoted-scalars
expect_pass reordered_json pass/reordered-json
expect_pass shell_script_invocation pass/shell-script-invocation

expect_fail action_tag fail/action-tag "floating or unapproved GitHub Action"
expect_fail action_without_comment fail/action-no-comment "approved release comment"
expect_fail action_bad_comment fail/action-bad-comment "approved release comment"
expect_fail quoted_action_tag fail/quoted-action-tag "floating or unapproved GitHub Action"
expect_fail flow_action_tag fail/flow-action-tag "floating or unapproved GitHub Action"
expect_fail second_inline_uses fail/second-inline-uses "floating or unapproved GitHub Action"
expect_fail ignored_workflow fail/ignored-workflow "hosted runner drift"
expect_fail wrong_job_setup_node fail/wrong-job-setup-node "setup-node is approved only in the markdownlint job"
expect_fail npx_long_option fail/npx-long-option "unapproved npm command"
expect_fail npx_short_option fail/npx-short-option "unapproved npm command"
expect_fail tla_execution_without_recheck fail/tla-no-run-check "TLA+ execution must verify"
expect_fail tla_download_without_checksum fail/tla-no-download-check "TLA+ download identity/checksum sequence drifted"
expect_fail tla_identity_drift fail/tla-identity-drift "TLA_TOOLS_VERSION must equal 1.7.4"
expect_fail npm_install fail/npm-install "unapproved npm command"
expect_fail package_version_drift fail/package-version "package.json must pin markdownlint-cli2"
expect_fail js_yaml_version_drift fail/js-yaml-version "package.json must pin js-yaml to 4.3.0"
expect_fail node_engine_upper_bound fail/node-engine-upper "package.json must constrain Node to >=22.23.1"
expect_fail setup_node_drift fail/setup-node "floating or unapproved GitHub Action"
expect_fail node_version_drift fail/node-version "Node version drift"
expect_fail image_tag_drift fail/image-tag "Postgres image identity drifted"
expect_fail image_digest_drift fail/image-digest "Postgres image identity drifted"
expect_fail store_postgres_tag_only fail/store-postgres-tag "testcontainers Postgres image identity drifted"
expect_fail grpc_postgres_tag_only fail/grpc-postgres-tag "testcontainers Postgres image identity drifted"
expect_fail store_postgres_digest_drift fail/store-postgres-digest "testcontainers Postgres image identity drifted"
expect_fail grpc_postgres_digest_drift fail/grpc-postgres-digest "testcontainers Postgres image identity drifted"
expect_fail postgres_pin_missing fail/postgres-pin-missing "testcontainers Postgres pin count drifted"
expect_fail postgres_pin_duplicate fail/postgres-pin-duplicate "testcontainers Postgres pin count drifted"
expect_fail postgres_pin_in_comment fail/postgres-pin-comment "testcontainers Postgres pin count drifted"
expect_fail postgres_pin_in_string fail/postgres-pin-string "testcontainers Postgres pin count drifted"
expect_fail image_without_digest fail/image-no-digest "container image must use a sha256 digest"
expect_fail multiple_inline_images fail/multiple-images "container image must use a sha256 digest"
expect_fail multiple_inline_runners fail/multiple-runs-on "hosted runner drift"
expect_fail unapproved_curl fail/unapproved-curl "unapproved executable download"
expect_fail unapproved_wget fail/unapproved-wget "unapproved executable download"
expect_fail env_wrapped_download fail/download-env-wrapper "unapproved executable download"
expect_fail command_wrapped_download fail/download-command-wrapper "unapproved executable download"
expect_fail substituted_download fail/download-substitution "unapproved executable download"
expect_fail chained_download fail/download-chain "unapproved executable download"
expect_fail backtick_download fail/download-backtick "unapproved executable download"
expect_fail quote_splice_download fail/download-quote-splice "unapproved executable download"
expect_fail backslash_splice_download fail/download-backslash-splice "unapproved executable download"
expect_fail line_continuation_download fail/download-line-continuation "unapproved executable download"
expect_fail local_composite_action fail/local-composite-action "local composite action is unscanned and forbidden"
expect_fail crate_publish_drift fail/publish-drift "crate manifest must set publish = false"
expect_fail npm_command_drift fail/npm-command "unapproved npm command"
expect_fail duplicate_json_key fail/duplicate-json "duplicate JSON key"
expect_fail invalid_yaml fail/invalid-yaml "YAML parse failed"
expect_fail invalid_json fail/invalid-json "JSON parse failed"
expect_fail lock_integrity_drift fail/lock-integrity "invalid package-lock integrity"
expect_fail lock_registry_drift fail/lock-registry "invalid package-lock registry source"
expect_fail lock_install_script fail/lock-script "package-lock contains install scripts"
expect_fail legal_file_drift fail/legal-drift "crate legal file drifted from root NOTICE"
expect_fail cargo_install_missing_locked fail/cargo-install-missing-locked "unapproved cargo install"
expect_fail cargo_install_wrapper fail/cargo-install-wrapper "unapproved cargo install"
expect_fail setup_java_latest fail/setup-java-latest "Java version drift"
expect_fail shell_indirection fail/shell-indirection "shell command indirection is forbidden"
expect_fail repository_script_download fail/repository-script "unapproved executable download"
expect_fail duplicate_tla_target fail/duplicate-tla-target "duplicate Makefile target: tla"
expect_fail shell_option_before_command fail/shell-option-before-command "shell command indirection is forbidden"
expect_fail shell_command_cluster fail/shell-command-cluster "shell command indirection is forbidden"
expect_fail cargo_toolchain_selector fail/cargo-toolchain-selector "unapproved cargo install"
expect_fail rustup_run_cargo fail/rustup-run-cargo "unapproved cargo install"
expect_fail dynamic_make_target fail/dynamic-make-target "dynamic Makefile target is forbidden"

fixture_root
shell_scanner_root=$fixture_path
mkdir -p "$shell_scanner_root/scripts"
ln -s ../Makefile "$shell_scanner_root/scripts/symlink.sh"
if "$gate" --root "$shell_scanner_root" >"$shell_scanner_root/output" 2>&1; then
  echo "FAIL shell_scanner_error: gate accepted a shell-script scanner failure" >&2
  exit 1
fi
if ! grep -Fq "filesystem scan failed: symlink is unsupported" "$shell_scanner_root/output"; then
  echo "FAIL shell_scanner_error: missing diagnostic" >&2
  exit 1
fi
echo "PASS shell_scanner_error"

fixture_root
filesystem_root=$fixture_path
unlink "$filesystem_root/compose.yaml"
if "$gate" --root "$filesystem_root" >"$filesystem_root/output" 2>&1; then
  echo "FAIL filesystem_error: gate accepted a missing required file" >&2
  exit 1
fi
if ! grep -Fq "filesystem scan failed" "$filesystem_root/output"; then
  echo "FAIL filesystem_error: missing diagnostic" >&2
  exit 1
fi
echo "PASS filesystem_error"

fixture_root
missing_legal_root=$fixture_path
unlink "$missing_legal_root/crates/koine-cli/NOTICE"
if "$gate" --root "$missing_legal_root" >"$missing_legal_root/output" 2>&1; then
  echo "FAIL missing_legal_file: gate accepted a missing crate NOTICE" >&2
  exit 1
fi
if ! grep -Fq "filesystem scan failed" "$missing_legal_root/output"; then
  echo "FAIL missing_legal_file: missing diagnostic" >&2
  exit 1
fi
echo "PASS missing_legal_file"

fixture_root
symlink_legal_root=$fixture_path
unlink "$symlink_legal_root/crates/koine-cli/NOTICE"
ln -s ../../NOTICE "$symlink_legal_root/crates/koine-cli/NOTICE"
if "$gate" --root "$symlink_legal_root" >"$symlink_legal_root/output" 2>&1; then
  echo "FAIL symlink_legal_file: gate accepted a symlinked crate NOTICE" >&2
  exit 1
fi
if ! grep -Fq "not a regular file" "$symlink_legal_root/output"; then
  echo "FAIL symlink_legal_file: missing diagnostic" >&2
  exit 1
fi
echo "PASS symlink_legal_file"

fixture_root
symlink_crate_root=$fixture_path
mv "$symlink_crate_root/crates/koine-cli" "$symlink_crate_root/koine-cli-real"
ln -s ../koine-cli-real "$symlink_crate_root/crates/koine-cli"
if "$gate" --root "$symlink_crate_root" >"$symlink_crate_root/output" 2>&1; then
  echo "FAIL symlink_crate_directory: gate accepted a symlinked crate directory" >&2
  exit 1
fi
if ! grep -Fq "workspace crate entry must be a real directory" "$symlink_crate_root/output"; then
  echo "FAIL symlink_crate_directory: missing diagnostic" >&2
  exit 1
fi
echo "PASS symlink_crate_directory"

fixture_root
non_directory_crate_root=$fixture_path
touch "$non_directory_crate_root/crates/unexpected-file"
if "$gate" --root "$non_directory_crate_root" >"$non_directory_crate_root/output" 2>&1; then
  echo "FAIL non_directory_crate_entry: gate accepted a non-directory crate entry" >&2
  exit 1
fi
if ! grep -Fq "workspace crate entry must be a real directory" "$non_directory_crate_root/output"; then
  echo "FAIL non_directory_crate_entry: missing diagnostic" >&2
  exit 1
fi
echo "PASS non_directory_crate_entry"

fixture_root
missing_crate_root=$fixture_path
mv "$missing_crate_root/crates/koine-cli" "$missing_crate_root/koine-cli-missing"
if "$gate" --root "$missing_crate_root" >"$missing_crate_root/output" 2>&1; then
  echo "FAIL missing_crate_directory: gate accepted a missing workspace crate" >&2
  exit 1
fi
if ! grep -Fq "workspace crate directory set drifted" "$missing_crate_root/output"; then
  echo "FAIL missing_crate_directory: missing diagnostic" >&2
  exit 1
fi
echo "PASS missing_crate_directory"

fixture_root
extra_crate_root=$fixture_path
mkdir "$extra_crate_root/crates/koine-extra"
if "$gate" --root "$extra_crate_root" >"$extra_crate_root/output" 2>&1; then
  echo "FAIL extra_crate_directory: gate accepted an extra workspace crate" >&2
  exit 1
fi
if ! grep -Fq "workspace crate directory set drifted" "$extra_crate_root/output"; then
  echo "FAIL extra_crate_directory: missing diagnostic" >&2
  exit 1
fi
echo "PASS extra_crate_directory"

fixture_root
missing_node_root=$fixture_path
empty_path=$(mktemp -d)
temporary_roots+=("$empty_path")
if PATH="$empty_path" /usr/bin/bash "$gate" --root "$missing_node_root" \
  >"$missing_node_root/output" 2>&1; then
  echo "FAIL missing_node: wrapper accepted a missing Node runtime" >&2
  exit 1
fi
if ! /usr/bin/grep -Fq "required Node runtime not found" "$missing_node_root/output"; then
  echo "FAIL missing_node: missing diagnostic" >&2
  exit 1
fi
echo "PASS missing_node"

isolated_root=$(mktemp -d)
temporary_roots+=("$isolated_root")
mkdir -p "$isolated_root/.github/scripts"
cp "$repository_root/.github/scripts/check-supply-chain.sh" "$isolated_root/.github/scripts/"
cp "$repository_root/.github/scripts/check-supply-chain.mjs" "$isolated_root/.github/scripts/"
if "$isolated_root/.github/scripts/check-supply-chain.sh" --root "$repository_root" \
  >"$isolated_root/output" 2>&1; then
  echo "FAIL missing_module: wrapper accepted a missing parser module" >&2
  exit 1
fi
if ! grep -Fq "required parser module missing" "$isolated_root/output"; then
  echo "FAIL missing_module: missing diagnostic" >&2
  exit 1
fi
echo "PASS missing_module"

mkdir -p "$isolated_root/node_modules/js-yaml"
printf '{"name":"js-yaml","type":"module","main":"index.js"}\n' \
  >"$isolated_root/node_modules/js-yaml/package.json"
printf 'this is not valid JavaScript\n' >"$isolated_root/node_modules/js-yaml/index.js"
if "$isolated_root/.github/scripts/check-supply-chain.sh" --root "$repository_root" \
  >"$isolated_root/output" 2>&1; then
  echo "FAIL import_error: checker accepted a parser import failure" >&2
  exit 1
fi
echo "PASS import_error"

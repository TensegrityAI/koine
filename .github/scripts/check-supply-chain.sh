#!/usr/bin/env bash
set -euo pipefail

usage() {
  echo "usage: $0 [--root REPOSITORY_ROOT]" >&2
}

repository_root=.
if (($# > 0)); then
  if (($# != 2)) || [[ $1 != --root ]]; then
    usage
    exit 2
  fi
  repository_root=${2%/}
fi

if [[ ! -d "$repository_root" ]]; then
  echo "repository root is not a directory: $repository_root" >&2
  exit 2
fi
if ! command -v rg >/dev/null 2>&1; then
  echo "required scanner 'rg' not found" >&2
  exit 2
fi

readonly workflows_dir="$repository_root/.github/workflows"
readonly scripts_dir="$repository_root/.github/scripts"
readonly makefile="$repository_root/Makefile"
readonly compose_file="$repository_root/compose.yaml"
readonly package_json="$repository_root/package.json"
readonly package_lock="$repository_root/package-lock.json"
readonly -a required_paths=(
  "$workflows_dir"
  "$scripts_dir"
  "$makefile"
  "$compose_file"
  "$package_json"
  "$package_lock"
)

for path in "${required_paths[@]}"; do
  if [[ ! -e "$path" || ! -r "$path" ]]; then
    echo "required scan path is missing or unreadable: ${path#"$repository_root"/}" >&2
    exit 2
  fi
done

scan_output=
if scan_output=$(rg --files --hidden "$workflows_dir" "$scripts_dir" 2>&1); then
  :
else
  scanner_status=$?
  echo "scanner failed while enumerating repository-owned inputs (status $scanner_status)" >&2
  [[ -z "$scan_output" ]] || echo "$scan_output" >&2
  exit 2
fi

declare -a scanned_files=("$makefile" "$compose_file")
if [[ -n "$scan_output" ]]; then
  while IFS= read -r path; do
    scanned_files+=("$path")
  done <<<"$scan_output"
fi

trim() {
  trimmed=$1
  trimmed=${trimmed#"${trimmed%%[![:space:]]*}"}
  trimmed=${trimmed%"${trimmed##*[![:space:]]}"}
}

split_comment() {
  local line=$1
  local char
  local escaped=0
  local in_single=0
  local in_double=0
  local index
  yaml_code=$line
  yaml_comment=

  for ((index = 0; index < ${#line}; index++)); do
    char=${line:index:1}
    if ((in_double)); then
      if ((escaped)); then
        escaped=0
      elif [[ $char == \\ ]]; then
        escaped=1
      elif [[ $char == '"' ]]; then
        in_double=0
      fi
    elif ((in_single)); then
      if [[ $char == "'" ]]; then
        in_single=0
      fi
    else
      case "$char" in
        '"') in_double=1 ;;
        "'") in_single=1 ;;
        '#')
          yaml_code=${line:0:index}
          yaml_comment=${line:index+1}
          break
          ;;
      esac
    fi
  done

  trim "$yaml_code"
  yaml_code=$trimmed
  trim "$yaml_comment"
  yaml_comment=$trimmed
}

status=0
fail() {
  echo "$*" >&2
  status=1
}

action_count_setup_node=0
setup_node_line=0
node_version_line=0
node_cache_line=0
workflow_npm_ci_line=0
workflow_npm_exec_line=0
tla_download_line=0
tla_download_checksum_line=0
tla_move_line=0
tla_run_checksum_line=0
tla_execution_line=0
gitleaks_download_line=0
gitleaks_checksum_line=0
postgres_exception_count=0
npm_ci_count=0
npm_exec_count=0

check_action() {
  local file=$1
  local line_number=$2
  local code=$3
  local comment=$4
  local uses_re="(^|[[:space:]{,-])('uses'|\"uses\"|uses)[[:space:]]*:[[:space:]]*"
  local matched
  local rest
  local target
  local tail
  local expected_comment=
  local char
  local index

  if [[ ! $code =~ $uses_re ]]; then
    return
  fi
  matched=${BASH_REMATCH[0]}
  rest=${code#*"$matched"}
  trim "$rest"
  rest=$trimmed

  if [[ $rest == '"'* ]]; then
    rest=${rest:1}
    if [[ $rest != *'"'* ]]; then
      fail "unsupported quoted uses value: $file:$line_number"
      return
    fi
    target=${rest%%\"*}
    tail=${rest#*\"}
  elif [[ $rest == "'"* ]]; then
    rest=${rest:1}
    if [[ $rest != *"'"* ]]; then
      fail "unsupported quoted uses value: $file:$line_number"
      return
    fi
    target=${rest%%\'*}
    tail=${rest#*\'}
  else
    target=
    for ((index = 0; index < ${#rest}; index++)); do
      char=${rest:index:1}
      if [[ $char =~ [[:space:],}] ]]; then
        break
      fi
      target+=$char
    done
    tail=${rest:${#target}}
  fi

  trim "$tail"
  tail=$trimmed
  if [[ -n "$tail" && $tail != ','* && $tail != '}'* ]]; then
    fail "unsupported uses syntax: $file:$line_number"
    return
  fi
  if [[ $target == ./* ]]; then
    return
  fi

  case "$target" in
    actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1)
      expected_comment=v7.0.1
      ;;
    actions-rust-lang/setup-rust-toolchain@166cdcfd11aee3cb47222f9ddb555ce30ddb9659)
      expected_comment=v1.17.0
      ;;
    EmbarkStudios/cargo-deny-action@3c6349835b2b7b196a839186cb8b78e02f7b5f25)
      expected_comment=v2.1.1
      ;;
    crate-ci/typos@bee27e3a4fd1ea2111cf90ab89cd076c870fce14)
      expected_comment=v1.48.0
      ;;
    actions/setup-java@03ad4de0992f5dab5e18fcb136590ce7c4a0ac95)
      expected_comment=v5.6.0
      ;;
    actions/setup-node@a0853c24544627f65ddf259abe73b1d18a591444)
      expected_comment=v5.0.0
      if [[ $file != .github/workflows/ci.yml ]]; then
        fail "setup-node is approved only in the markdownlint CI workflow: $file:$line_number"
      else
        ((action_count_setup_node += 1))
        setup_node_line=$line_number
      fi
      ;;
    *)
      fail "floating or unapproved GitHub Action: $file:$line_number: $target"
      return
      ;;
  esac

  if [[ $comment != "$expected_comment" ]]; then
    fail "GitHub Action lacks its approved release comment '$expected_comment': $file:$line_number"
  fi
}

check_image() {
  local file=$1
  local line_number=$2
  local code=$3
  local image_re='(^|[[:space:]{,-])image[[:space:]]*:[[:space:]]*([^,}[:space:]]+)'
  local image

  if [[ ! $code =~ $image_re ]]; then
    return
  fi
  image=${BASH_REMATCH[2]}
  image=${image#\"}
  image=${image%\"}
  image=${image#\'}
  image=${image%\'}

  if [[ $file == compose.yaml && $image == postgres:17 ]]; then
    ((postgres_exception_count += 1))
  elif [[ $file == compose.yaml && $image == postgres:* ]]; then
    fail "temporary Postgres image exception drifted: $file:$line_number: $image"
  elif [[ ! $image =~ @sha256:[0-9a-f]{64}$ ]]; then
    fail "container image must use a sha256 digest: $file:$line_number: $image"
  fi
}

for absolute_file in "${scanned_files[@]}"; do
  if [[ ! -f "$absolute_file" || ! -r "$absolute_file" ]]; then
    fail "scan input became missing or unreadable: ${absolute_file#"$repository_root"/}"
    continue
  fi
  relative_file=${absolute_file#"$repository_root"/}
  contents=
  if contents=$(<"$absolute_file"); then
    :
  else
    fail "scan input read failed: $relative_file"
    continue
  fi

  line_number=0
  while IFS= read -r line || [[ -n "$line" ]]; do
    ((line_number += 1))
    split_comment "$line"
    code=$yaml_code
    comment=$yaml_comment
    [[ -z "$code" ]] && continue

    if [[ $relative_file == .github/workflows/* ]]; then
      check_action "$relative_file" "$line_number" "$code" "$comment"
      if [[ $code =~ (^|[[:space:]])runs-on[[:space:]]*:[[:space:]]*([^[:space:],}]+) ]]; then
        runner=${BASH_REMATCH[2]}
        runner=${runner#\"}
        runner=${runner%\"}
        if [[ $runner != ubuntu-24.04 ]]; then
          fail "hosted runner drift: $relative_file:$line_number: $runner"
        fi
      fi
      if [[ $code =~ node-version[[:space:]]*:[[:space:]]*([^[:space:],}]+) ]]; then
        node_version=${BASH_REMATCH[1]}
        node_version=${node_version#\"}
        node_version=${node_version%\"}
        if [[ $node_version != 22.23.1 ]]; then
          fail "Node version drift: $relative_file:$line_number: $node_version"
        else
          node_version_line=$line_number
        fi
      fi
      if [[ $code =~ package-manager-cache[[:space:]]*:[[:space:]]*([^[:space:],}]+) ]]; then
        node_cache=${BASH_REMATCH[1]}
        if [[ $node_cache != false ]]; then
          fail "setup-node package-manager-cache must be false: $relative_file:$line_number"
        else
          node_cache_line=$line_number
        fi
      fi
    fi

    check_image "$relative_file" "$line_number" "$code"

    command=$code
    if [[ $command =~ ^-[[:space:]]*run:[[:space:]]*(.*)$ ]]; then
      command=${BASH_REMATCH[1]}
      trim "$command"
      command=$trimmed
    fi

    if [[ $command =~ (^|[\;\|\&])[[:space:]]*npx[[:space:]]+(-y|--yes)([[:space:]]|$) ]]; then
      fail "floating npm execution: $relative_file:$line_number"
    fi
    if [[ $command =~ (^|[\;\|\&])[[:space:]]*npm[[:space:]]+install([[:space:]]|$) ]]; then
      fail "npm install is not allowed; use npm ci --ignore-scripts: $relative_file:$line_number"
    fi
    if [[ $command == 'npm ci --ignore-scripts' ]]; then
      ((npm_ci_count += 1))
      if [[ $relative_file == .github/workflows/ci.yml ]]; then
        workflow_npm_ci_line=$line_number
      fi
    fi
    if [[ $command == npm\ exec\ --\ markdownlint-cli2* ]]; then
      ((npm_exec_count += 1))
      if [[ $relative_file == .github/workflows/ci.yml ]]; then
        workflow_npm_exec_line=$line_number
      fi
    fi
    if [[ $command == cargo\ install* ]]; then
      if [[ ! $command =~ --version[[:space:]]+[0-9]+\.[0-9]+\.[0-9]+([[:space:]]|$) || ! $command =~ (^|[[:space:]])--locked([[:space:]]|$) ]]; then
        fail "cargo install must use an exact version and --locked: $relative_file:$line_number"
      fi
    fi

    if [[ $command =~ (^|[\;\|\&])[[:space:]]*(curl|wget)[[:space:]] ]]; then
      case "$command" in
        'curl -fsSL $(TLA_TOOLS_URL) -o $(TLA_TOOLS).tmp')
          if [[ $relative_file == Makefile ]]; then
            tla_download_line=$line_number
          else
            fail "unapproved executable download: $relative_file:$line_number"
          fi
          ;;
        'curl -fsSL "https://github.com/gitleaks/gitleaks/releases/download/v${GITLEAKS_VERSION}/gitleaks_${GITLEAKS_VERSION}_linux_x64.tar.gz" -o gitleaks.tgz')
          if [[ $relative_file == .github/workflows/ci.yml ]]; then
            gitleaks_download_line=$line_number
          else
            fail "unapproved executable download: $relative_file:$line_number"
          fi
          ;;
        *)
          fail "unapproved executable download: $relative_file:$line_number"
          ;;
      esac
    fi

    case "$command" in
      'echo "$(TLA_TOOLS_SHA256)  $(TLA_TOOLS).tmp" | sha256sum -c')
        if [[ $relative_file == Makefile ]]; then
          tla_download_checksum_line=$line_number
        fi
        ;;
      'mv $(TLA_TOOLS).tmp $(TLA_TOOLS)')
        if [[ $relative_file == Makefile ]]; then
          tla_move_line=$line_number
        fi
        ;;
      'echo "$(TLA_TOOLS_SHA256)  $(TLA_TOOLS)" | sha256sum -c')
        if [[ $relative_file == Makefile ]]; then
          tla_run_checksum_line=$line_number
        fi
        ;;
      'echo "9991e0b2903da4c8f6122b5c3186448b927a5da4deef1fe45271c3793f4ee29c  gitleaks.tgz" | sha256sum -c')
        if [[ $relative_file == .github/workflows/ci.yml ]]; then
          gitleaks_checksum_line=$line_number
        fi
        ;;
    esac
    if [[ $command =~ (^|[\;\|\&])[[:space:]]*java[[:space:]].*tla2tools\.jar ]]; then
      if [[ $relative_file == Makefile ]]; then
        tla_execution_line=$line_number
      else
        fail "unapproved TLA+ execution: $relative_file:$line_number"
      fi
    fi
  done <<<"$contents"
done

package_contents=$(<"$package_json") || {
  echo "package.json read failed" >&2
  exit 2
}
lock_contents=$(<"$package_lock") || {
  echo "package-lock.json read failed" >&2
  exit 2
}

if [[ ! $package_contents =~ \"packageManager\"[[:space:]]*:[[:space:]]*\"npm@10\.9\.8\" ]]; then
  fail "package.json must pin packageManager to npm@10.9.8"
fi
if [[ ! $package_contents =~ \"node\"[[:space:]]*:[[:space:]]*\"\>=22\.23\.1\ \<23\" ]]; then
  fail "package.json must constrain Node to >=22.23.1 <23"
fi
if [[ ! $package_contents =~ \"markdownlint-cli2\"[[:space:]]*:[[:space:]]*\"0\.23\.1\" ]]; then
  fail "package.json must pin markdownlint-cli2 to 0.23.1"
fi
if [[ $package_contents =~ \"scripts\"[[:space:]]*: || $lock_contents =~ \"hasInstallScript\"[[:space:]]*: ]]; then
  fail "repository tooling must not declare npm lifecycle scripts"
fi
if [[ ! $lock_contents =~ \"markdownlint-cli2\"[[:space:]]*:[[:space:]]*\"0\.23\.1\" || ! $lock_contents =~ \"version\"[[:space:]]*:[[:space:]]*\"0\.23\.1\" ]]; then
  fail "package-lock.json must resolve markdownlint-cli2 0.23.1"
fi

if ((action_count_setup_node != 1)); then
  fail "markdownlint CI must contain exactly one approved setup-node action"
fi
if ((node_version_line == 0)); then
  fail "markdownlint CI must pin node-version 22.23.1"
fi
if ((node_cache_line == 0)); then
  fail "markdownlint CI must disable setup-node package-manager-cache"
fi
if ((workflow_npm_ci_line == 0 || workflow_npm_exec_line == 0)); then
  fail "markdownlint CI must use npm ci --ignore-scripts followed by npm exec"
elif ! ((setup_node_line < node_version_line && node_version_line <= node_cache_line && node_cache_line < workflow_npm_ci_line && workflow_npm_ci_line < workflow_npm_exec_line)); then
  fail "setup-node and exact Node inputs must precede markdownlint npm execution"
fi
if ((npm_ci_count != 2 || npm_exec_count != 2)); then
  fail "Makefile and CI must each use one approved npm ci/npm exec pair"
fi

if ((tla_download_line == 0 || tla_download_checksum_line <= tla_download_line || tla_move_line <= tla_download_checksum_line)); then
  fail "TLA+ download must verify its checksum before installation"
fi
if ((tla_execution_line == 0 || tla_run_checksum_line == 0 || tla_run_checksum_line >= tla_execution_line)); then
  fail "TLA+ execution must verify its checksum immediately beforehand"
fi
if ((gitleaks_download_line == 0 || gitleaks_checksum_line <= gitleaks_download_line)); then
  fail "gitleaks download must verify its approved checksum"
fi
if ((postgres_exception_count != 1)); then
  fail "compose.yaml must contain the one temporary postgres:17 exception owned by Operational Task 4"
fi

exit "$status"

#!/usr/bin/env node
import { lstat, readdir, readFile } from "node:fs/promises";
import path from "node:path";
import process from "node:process";
import yaml from "js-yaml";

const APPROVED_ACTIONS = new Map([
  ["actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1", "v7.0.1"],
  ["actions-rust-lang/setup-rust-toolchain@166cdcfd11aee3cb47222f9ddb555ce30ddb9659", "v1.17.0"],
  ["EmbarkStudios/cargo-deny-action@3c6349835b2b7b196a839186cb8b78e02f7b5f25", "v2.1.1"],
  ["crate-ci/typos@bee27e3a4fd1ea2111cf90ab89cd076c870fce14", "v1.48.0"],
  ["actions/setup-java@03ad4de0992f5dab5e18fcb136590ce7c4a0ac95", "v5.6.0"],
  ["actions/setup-node@a0853c24544627f65ddf259abe73b1d18a591444", "v5.0.0"],
]);
const CHECKOUT = "actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1";
const SETUP_JAVA = "actions/setup-java@03ad4de0992f5dab5e18fcb136590ce7c4a0ac95";
const SETUP_NODE = "actions/setup-node@a0853c24544627f65ddf259abe73b1d18a591444";
const CARGO_MACHETE_INSTALL = "cargo install cargo-machete --version 0.9.2 --locked";
const MARKDOWN_COMMAND = 'npm exec -- markdownlint-cli2 "**/*.md" "!_archive" "!target" "!node_modules" "!docs/superpowers" "!.superpowers" "!.worktrees"';
const GITLEAKS_CHECKSUM_LINE = 'echo "9991e0b2903da4c8f6122b5c3186448b927a5da4deef1fe45271c3793f4ee29c  gitleaks.tgz" | sha256sum -c';
const GITLEAKS_EXACT_COMMAND = [
  'curl -fsSL "https://github.com/gitleaks/gitleaks/releases/download/v${GITLEAKS_VERSION}/gitleaks_${GITLEAKS_VERSION}_linux_x64.tar.gz" -o gitleaks.tgz',
  GITLEAKS_CHECKSUM_LINE,
  "tar -xzf gitleaks.tgz gitleaks",
  "./gitleaks detect --source . --redact --verbose",
].join("\n");
const TLA_VERSION = "1.7.4";
const TLA_SHA256 = "936a262061c914694dfd669a543be24573c45d5aa0ff20a8b96b23d01e050e88";
const TLA_URL = "https://github.com/tlaplus/tlaplus/releases/download/v$(TLA_TOOLS_VERSION)/tla2tools.jar";
const JS_YAML_INTEGRITY = "sha512-1td788aAnnZ5qs7V2QIRl1owjtYpbKt749Y3xauqQgwIIGF/xXWz1wMTEBx5O3LK3lXLVuqXPdPxj2BoFHaW9Q==";
const MARKDOWNLINT_INTEGRITY = "sha512-20JPI5W+HpV1OA+pUM712wgvL4GzYNUvbmhLU8KlEYJ1kCDx4soZ4/Xqd+WkLrPTOKMAn8SfO3zYFrK8GLlwQg==";
const POSTGRES_IMAGE = "postgres:17@sha256:a426e44bac0b759c95894d68e1a0ac03ecc20b619f498a91aae373bf06d8508d";
const POSTGRES_TESTCONTAINERS_TAG = "17@sha256:a426e44bac0b759c95894d68e1a0ac03ecc20b619f498a91aae373bf06d8508d";
const POSTGRES_TESTCONTAINERS_HELPERS = [
  "crates/koine-store-postgres/tests/support/mod.rs",
  "crates/koine-grpc/tests/support/mod.rs",
];
const WORKSPACE_CRATES = [
  "koine-application",
  "koine-cli",
  "koine-domain",
  "koine-grpc",
  "koine-http",
  "koine-mcp",
  "koine-observability",
  "koine-proto",
  "koine-server",
  "koine-store-memory",
  "koine-store-postgres",
];

function fail(message) {
  throw new Error(message);
}

function isObject(value) {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

function exactKeys(value, expected, label) {
  if (!isObject(value)) fail(`${label} must be an object`);
  const actual = Object.keys(value).sort();
  const wanted = [...expected].sort();
  if (JSON.stringify(actual) !== JSON.stringify(wanted)) {
    fail(`${label} keys drifted: ${actual.join(", ")}`);
  }
}

function versionAtLeast(actual, minimum) {
  const left = actual.split(".").map(Number);
  const right = minimum.split(".").map(Number);
  for (let index = 0; index < 3; index += 1) {
    if (left[index] > right[index]) return true;
    if (left[index] < right[index]) return false;
  }
  return true;
}

async function readText(file) {
  try {
    const metadata = await lstat(file);
    if (!metadata.isFile()) fail(`filesystem scan failed: not a regular file: ${file}`);
    return await readFile(file, "utf8");
  } catch (error) {
    if (error instanceof Error && error.message.startsWith("filesystem scan failed")) throw error;
    fail(`filesystem scan failed for ${file}: ${error.message}`);
  }
}

async function validateCrateLegalFiles(root) {
  const canonical = new Map([
    ["LICENSE", await readText(path.join(root, "LICENSE"))],
    ["NOTICE", await readText(path.join(root, "NOTICE"))],
  ]);
  const cratesRoot = path.join(root, "crates");
  let entries;
  try {
    entries = await readdir(cratesRoot, { withFileTypes: true });
  } catch (error) {
    fail(`filesystem scan failed for ${cratesRoot}: ${error.message}`);
  }
  entries.sort((left, right) => left.name.localeCompare(right.name));
  for (const entry of entries) {
    if (entry.isSymbolicLink() || !entry.isDirectory()) {
      fail(`workspace crate entry must be a real directory: crates/${entry.name}`);
    }
  }
  const actualCrates = entries.map((entry) => entry.name);
  if (JSON.stringify(actualCrates) !== JSON.stringify(WORKSPACE_CRATES)) {
    fail(`workspace crate directory set drifted: ${actualCrates.join(", ")}`);
  }
  for (const entry of entries) {
    const crateRoot = path.join(cratesRoot, entry.name);
    const manifest = await readText(path.join(crateRoot, "Cargo.toml"));
    // Every workspace crate must stay non-publishable: a crate silently
    // dropping `publish = false` could be pushed to crates.io before the
    // deliberate 2B publication decision. Enforce it, don't just assume it.
    if (!/^\s*publish\s*=\s*false\s*$/mu.test(manifest)) {
      fail(`crate manifest must set publish = false: crates/${entry.name}/Cargo.toml`);
    }
    for (const [name, expected] of canonical) {
      const file = path.join(crateRoot, name);
      const actual = await readText(file);
      if (actual !== expected) {
        const relative = path.relative(root, file).split(path.sep).join("/");
        fail(`crate legal file drifted from root ${name}: ${relative}`);
      }
    }
  }
}

const RUST_PUNCTUATION = new Set("{}[]();:,.#&<>+-*/=!?|^%@$".split(""));

function rustLexFail(label, index, message) {
  fail(`Rust helper lex failed: ${label}:${index}: ${message}`);
}

function rustRawStringHashes(source, index) {
  if (source[index] !== "r") return null;
  let cursor = index + 1;
  while (source[cursor] === "#") cursor += 1;
  return source[cursor] === '"' ? cursor - index - 1 : null;
}

function readRustRawString(source, index, hashes, label) {
  const contentStart = index + hashes + 2;
  const terminator = `"${"#".repeat(hashes)}`;
  const contentEnd = source.indexOf(terminator, contentStart);
  if (contentEnd === -1) rustLexFail(label, index, "unterminated raw string literal");
  return {
    next: contentEnd + terminator.length,
    token: { kind: "string", value: source.slice(contentStart, contentEnd) },
  };
}

function readRustString(source, index, label) {
  let cursor = index + 1;
  let value = "";
  const simpleEscapes = new Map([
    ["0", "\0"],
    ["t", "\t"],
    ["n", "\n"],
    ["r", "\r"],
    ['"', '"'],
    ["\\", "\\"],
  ]);
  while (cursor < source.length) {
    const character = source[cursor];
    if (character === '"') {
      return { next: cursor + 1, token: { kind: "string", value } };
    }
    if (character !== "\\") {
      value += character;
      cursor += 1;
      continue;
    }

    const escape = source[cursor + 1];
    if (simpleEscapes.has(escape)) {
      value += simpleEscapes.get(escape);
      cursor += 2;
      continue;
    }
    if (escape === "x") {
      const digits = source.slice(cursor + 2, cursor + 4);
      if (!/^[0-7][0-9a-f]$/iu.test(digits)) rustLexFail(label, cursor, "unsupported hex string escape");
      value += String.fromCodePoint(Number.parseInt(digits, 16));
      cursor += 4;
      continue;
    }
    if (escape === "u" && source[cursor + 2] === "{") {
      const close = source.indexOf("}", cursor + 3);
      const digits = close === -1 ? "" : source.slice(cursor + 3, close).replaceAll("_", "");
      if (!/^[0-9a-f]{1,6}$/iu.test(digits)) rustLexFail(label, cursor, "unsupported Unicode string escape");
      const codePoint = Number.parseInt(digits, 16);
      if (codePoint > 0x10ffff || (codePoint >= 0xd800 && codePoint <= 0xdfff)) {
        rustLexFail(label, cursor, "invalid Unicode string escape");
      }
      value += String.fromCodePoint(codePoint);
      cursor = close + 1;
      continue;
    }
    if (escape === "\n" || (escape === "\r" && source[cursor + 2] === "\n")) {
      cursor += escape === "\n" ? 2 : 3;
      while (/\s/u.test(source[cursor] ?? "")) cursor += 1;
      continue;
    }
    rustLexFail(label, cursor, "unsupported string escape");
  }
  rustLexFail(label, index, "unterminated string literal");
}

function lexRustHelper(source, label) {
  const tokens = [];
  let index = 0;
  while (index < source.length) {
    if (/\s/u.test(source[index])) {
      index += 1;
      continue;
    }
    if (source.startsWith("//", index)) {
      const newline = source.indexOf("\n", index + 2);
      index = newline === -1 ? source.length : newline + 1;
      continue;
    }
    if (source.startsWith("/*", index)) {
      const start = index;
      let depth = 1;
      index += 2;
      while (index < source.length && depth > 0) {
        if (source.startsWith("/*", index)) {
          depth += 1;
          index += 2;
        } else if (source.startsWith("*/", index)) {
          depth -= 1;
          index += 2;
        } else index += 1;
      }
      if (depth !== 0) rustLexFail(label, start, "unterminated block comment");
      continue;
    }
    if (source.startsWith("*/", index)) rustLexFail(label, index, "unmatched block-comment terminator");

    const rawHashes = rustRawStringHashes(source, index);
    if (rawHashes !== null) {
      const result = readRustRawString(source, index, rawHashes, label);
      tokens.push(result.token);
      index = result.next;
      continue;
    }
    if (/[bc]/u.test(source[index])
      && (source[index + 1] === '"' || rustRawStringHashes(source, index + 1) !== null)) {
      rustLexFail(label, index, "unsupported Rust string prefix");
    }
    if (source[index] === '"') {
      const result = readRustString(source, index, label);
      tokens.push(result.token);
      index = result.next;
      continue;
    }
    if (source[index] === "'") {
      rustLexFail(label, index, "unsupported Rust literal form");
    }
    if (/[A-Za-z_]/u.test(source[index])) {
      let cursor = index + 1;
      while (/[A-Za-z0-9_]/u.test(source[cursor] ?? "")) cursor += 1;
      tokens.push({ kind: "identifier", value: source.slice(index, cursor) });
      index = cursor;
      continue;
    }
    if (/[0-9]/u.test(source[index])) {
      let cursor = index + 1;
      while (/[A-Za-z0-9_]/u.test(source[cursor] ?? "")) cursor += 1;
      tokens.push({ kind: "number", value: source.slice(index, cursor) });
      index = cursor;
      continue;
    }
    if (RUST_PUNCTUATION.has(source[index])) {
      tokens.push({ kind: "punctuation", value: source[index] });
      index += 1;
      continue;
    }
    rustLexFail(label, index, `unsupported token ${JSON.stringify(source[index])}`);
  }
  return tokens;
}

function rustTokensMatch(tokens, index, expected) {
  return expected.every(([kind, value], offset) => {
    const token = tokens[index + offset];
    return token?.kind === kind && token.value === value;
  });
}

async function validatePostgresTestcontainersHelpers(root) {
  const postgresDefault = [
    ["identifier", "Postgres"],
    ["punctuation", ":"],
    ["punctuation", ":"],
    ["identifier", "default"],
    ["punctuation", "("],
    ["punctuation", ")"],
  ];
  const beforeTag = [
    ["punctuation", "."],
    ["identifier", "with_tag"],
    ["punctuation", "("],
  ];
  const afterTag = [
    ["punctuation", ")"],
    ["punctuation", "."],
    ["identifier", "start"],
    ["punctuation", "("],
  ];
  for (const relative of POSTGRES_TESTCONTAINERS_HELPERS) {
    const source = await readText(path.join(root, relative));
    const tokens = lexRustHelper(source, relative);
    const consumers = [];
    const pinnedConsumers = [];
    for (let index = 0; index < tokens.length; index += 1) {
      if (!rustTokensMatch(tokens, index, postgresDefault)) continue;
      consumers.push(index);
      let cursor = index + postgresDefault.length;
      if (!rustTokensMatch(tokens, cursor, beforeTag)) continue;
      cursor += beforeTag.length;
      const tag = tokens[cursor];
      if (tag?.kind !== "string") continue;
      cursor += 1;
      if (!rustTokensMatch(tokens, cursor, afterTag)) continue;
      pinnedConsumers.push({ index, tag: tag.value });
    }
    if (consumers.length !== 1 || pinnedConsumers.length !== 1 || consumers[0] !== pinnedConsumers[0].index) {
      fail(`testcontainers Postgres pin count drifted: ${relative}: ${pinnedConsumers.length}/${consumers.length}`);
    }
    if (pinnedConsumers[0].tag !== POSTGRES_TESTCONTAINERS_TAG) {
      fail(`testcontainers Postgres image identity drifted: ${relative}: ${pinnedConsumers[0].tag}`);
    }
  }
}

async function enumerateFiles(directory, predicate, skip = () => false) {
  const files = [];
  async function visit(current) {
    let entries;
    try {
      entries = await readdir(current, { withFileTypes: true });
    } catch (error) {
      fail(`filesystem scan failed for ${current}: ${error.message}`);
    }
    entries.sort((left, right) => left.name.localeCompare(right.name));
    for (const entry of entries) {
      const candidate = path.join(current, entry.name);
      if (skip(candidate, entry)) continue;
      if (entry.isSymbolicLink()) fail(`filesystem scan failed: symlink is unsupported: ${candidate}`);
      if (entry.isDirectory()) await visit(candidate);
      else if (entry.isFile() && predicate(candidate)) files.push(candidate);
    }
  }
  await visit(directory);
  return files;
}

function parseYaml(text, label) {
  try {
    const documents = [];
    yaml.loadAll(text, (document) => documents.push(document));
    if (documents.length !== 1) fail(`YAML parse failed for ${label}: exactly one document is required`);
    return documents[0];
  } catch (error) {
    if (error instanceof Error && error.message.startsWith("YAML parse failed")) throw error;
    fail(`YAML parse failed for ${label}: ${error.message}`);
  }
}

function parseJson(text, label) {
  try {
    yaml.load(text, { schema: yaml.JSON_SCHEMA });
  } catch (error) {
    if (/duplicated mapping key/i.test(error.message)) fail(`duplicate JSON key in ${label}: ${error.message}`);
    fail(`JSON parse failed for ${label}: ${error.message}`);
  }
  try {
    return JSON.parse(text);
  } catch (error) {
    fail(`JSON parse failed for ${label}: ${error.message}`);
  }
}

function validateActionSource(text, semanticActions, label) {
  const records = [];
  const actionLine = /^\s*-\s+uses:\s+([^\s#]+)(?:\s+#\s+(\S+))?\s*$/;
  for (const [index, line] of text.split(/\r?\n/u).entries()) {
    const match = line.match(actionLine);
    if (!match) continue;
    records.push({ target: match[1], comment: match[2] ?? "", line: index + 1 });
  }
  if (records.length !== semanticActions.length) {
    fail(`unsupported action syntax in ${label}; use one unquoted block '- uses:' per step`);
  }
  const semanticCounts = new Map();
  for (const action of semanticActions) semanticCounts.set(action, (semanticCounts.get(action) ?? 0) + 1);
  for (const record of records) {
    const count = semanticCounts.get(record.target) ?? 0;
    if (count === 0) fail(`action source/semantic mismatch in ${label}:${record.line}`);
    semanticCounts.set(record.target, count - 1);
    if (record.target.startsWith("./")) continue;
    const expectedComment = APPROVED_ACTIONS.get(record.target);
    if (!expectedComment) fail(`floating or unapproved GitHub Action: ${label}:${record.line}: ${record.target}`);
    if (record.comment !== expectedComment) {
      fail(`GitHub Action lacks its approved release comment '${expectedComment}': ${label}:${record.line}`);
    }
  }
}

function validateImage(image, location) {
  if (typeof image !== "string") fail(`container image must be a string: ${location}`);
  if (location === "compose.yaml:services.postgres" && image !== POSTGRES_IMAGE) {
    fail(`Postgres image identity drifted: ${location}: ${image}`);
  }
  if (!/@sha256:[0-9a-f]{64}$/u.test(image)) {
    fail(`container image must use a sha256 digest: ${location}: ${image}`);
  }
}

// Parse shell source into a list of simple commands (each an array of resolved
// words). This defeats quoting/escaping/continuation evasions of command
// detection: adjacent quoted and unquoted fragments concatenate into one word
// exactly as a shell would — `cu""rl`, `cu"r"l`, `cu\rl`, and a `\`+newline
// continuation all resolve to the word `curl` — while a wholly-quoted argument
// (`echo "cargo install ..."`) stays an argument, never a command. `$( )` and
// backtick substitutions have their inner text parsed as their own simple
// commands, so a nested `curl | bash` is still scanned. Detection then looks
// only at command position, which is both what the shell executes and what
// avoids false positives on words that merely appear inside string data.
//
// Not resolved: variable expansion. `${X}` stays literal, so an env-indirected
// command *name* (`FRAGMENT=cur; ${FRAGMENT}l`) is a documented residual limit
// of static analysis — it cannot be seen without executing the shell.
function parseShellCommands(source) {
  const commands = [];
  const subs = [];
  let current = [];
  let word = "";
  let hasWord = false;
  const length = source.length;
  const endWord = () => {
    if (hasWord) {
      current.push(word);
      word = "";
      hasWord = false;
    }
  };
  const endCommand = () => {
    endWord();
    if (current.length > 0) {
      commands.push(current);
      current = [];
    }
  };
  // Reads a `$( ... )` region from just after `$(`, balancing nested parens,
  // parsing the inner command list, returns the next index.
  const readParen = (from) => {
    let depth = 1;
    let inner = "";
    let index = from;
    while (index < length && depth > 0) {
      const char = source[index];
      if (char === "\\" && index + 1 < length) {
        inner += char + source[index + 1];
        index += 2;
        continue;
      }
      if (char === "(") depth += 1;
      else if (char === ")") {
        depth -= 1;
        if (depth === 0) {
          index += 1;
          break;
        }
      }
      inner += char;
      index += 1;
    }
    subs.push(...parseShellCommands(inner));
    return index;
  };
  // Reads a backtick region from just after the opening backtick.
  const readBacktick = (from) => {
    let inner = "";
    let index = from;
    while (index < length) {
      const char = source[index];
      if (char === "\\" && index + 1 < length) {
        inner += source[index + 1];
        index += 2;
        continue;
      }
      if (char === "`") {
        index += 1;
        break;
      }
      inner += char;
      index += 1;
    }
    subs.push(...parseShellCommands(inner));
    return index;
  };
  let i = 0;
  while (i < length) {
    const char = source[i];
    const next = source[i + 1];
    if (char === " " || char === "\t") {
      endWord();
      i += 1;
      continue;
    }
    if (char === "\n" || char === "\r") {
      endCommand();
      i += 1;
      continue;
    }
    if (char === "\\" && next === "\n") {
      i += 2;
      continue;
    }
    if (char === "\\" && next === "\r" && source[i + 2] === "\n") {
      i += 3;
      continue;
    }
    if (char === "\\" && next !== undefined) {
      word += next;
      hasWord = true;
      i += 2;
      continue;
    }
    if (char === "'") {
      hasWord = true;
      i += 1;
      while (i < length && source[i] !== "'") {
        word += source[i];
        i += 1;
      }
      i += 1;
      continue;
    }
    if (char === '"') {
      hasWord = true;
      i += 1;
      while (i < length && source[i] !== '"') {
        const inner = source[i];
        if (inner === "\\" && i + 1 < length) {
          word += source[i + 1];
          i += 2;
          continue;
        }
        if (inner === "$" && source[i + 1] === "(") {
          i = readParen(i + 2);
          continue;
        }
        if (inner === "`") {
          i = readBacktick(i + 1);
          continue;
        }
        word += inner;
        i += 1;
      }
      i += 1;
      continue;
    }
    if (char === "$" && next === "(") {
      hasWord = true;
      i = readParen(i + 2);
      continue;
    }
    if (char === "`") {
      hasWord = true;
      i = readBacktick(i + 1);
      continue;
    }
    if (char === "#" && !hasWord) {
      while (i < length && source[i] !== "\n") i += 1;
      continue;
    }
    if (char === ";" || char === "&" || char === "|" || char === "(" || char === ")") {
      endCommand();
      i += 1;
      continue;
    }
    word += char;
    hasWord = true;
    i += 1;
  }
  endCommand();
  return [...commands, ...subs];
}

const COMMAND_ASSIGNMENT = /^[A-Za-z_][A-Za-z0-9_]*=/u;
// Prefixes that run the command that follows them (so the effective command is
// further along): `command curl`, `env X=y curl`, `sudo curl`, `xargs curl`…
const COMMAND_RUNNERS = new Set([
  "env", "command", "sudo", "nohup", "nice", "xargs", "timeout", "setsid",
  "stdbuf", "time", "exec", "builtin", "then", "do", "!",
]);

function basename(token) {
  return token.split("/").at(-1);
}

// From a simple command's words, strip leading `VAR=value` assignments and
// runner prefixes to reach the effective command name and its arguments.
function effectiveWords(words) {
  let index = 0;
  while (index < words.length && COMMAND_ASSIGNMENT.test(words[index])) index += 1;
  for (;;) {
    if (index >= words.length) break;
    const head = basename(words[index]);
    // `rustup run <toolchain> <cmd...>` executes <cmd> — treat it as a runner.
    if (head === "rustup" && basename(words[index + 1] ?? "") === "run") {
      index += 2;
      while (index < words.length && words[index].startsWith("-")) index += 1;
      if (index < words.length) index += 1;
      continue;
    }
    if (!COMMAND_RUNNERS.has(head)) break;
    index += 1;
    while (index < words.length && words[index].startsWith("-")) index += 1;
    if (head === "env") {
      while (index < words.length && COMMAND_ASSIGNMENT.test(words[index])) index += 1;
    }
    if (head === "timeout" && index < words.length && !words[index].startsWith("-")) index += 1;
  }
  return words.slice(index);
}

function effectiveCommands(source) {
  return parseShellCommands(source)
    .map(effectiveWords)
    .filter((words) => words.length > 0);
}

function containsCommand(source, command) {
  return effectiveCommands(source).some((words) => basename(words[0]) === command);
}

function containsCargoInstall(source) {
  return effectiveCommands(source).some((words) => {
    if (basename(words[0]) !== "cargo") return false;
    let index = 1;
    if (words[index]?.startsWith("+")) index += 1;
    return words[index] === "install";
  });
}

function containsShellIndirection(source) {
  const shells = new Set(["bash", "sh", "zsh", "dash"]);
  return effectiveCommands(source).some((words) => {
    if (!shells.has(basename(words[0]))) return false;
    for (const option of words.slice(1)) {
      if (option === "--") break;
      if (!option.startsWith("-") && !option.startsWith("+")) break;
      if (option === "--command" || /^-[^-]*c/u.test(option)) return true;
    }
    return false;
  });
}

// Yields logical lines for line-oriented scanners: physical lines ending in an
// odd number of backslashes are continuations and are joined with the next
// line (numbered by the first physical line), so a `\`+newline split of a
// command — `cu\`<newline>`rl` — cannot hide it from a per-line scan.
function* logicalLines(text) {
  const physical = text.split(/\r?\n/u);
  let buffer = "";
  let startLine = 1;
  for (let index = 0; index < physical.length; index += 1) {
    const line = physical[index];
    if (buffer === "") startLine = index + 1;
    const trailing = line.match(/(\\*)$/u)[1].length;
    if (trailing % 2 === 1) {
      buffer += line.slice(0, -1);
      continue;
    }
    yield { value: buffer + line, number: startLine };
    buffer = "";
  }
  if (buffer !== "") yield { value: buffer, number: startLine };
}

function validateShellCommands(command, location) {
  if (containsShellIndirection(command)) fail(`shell command indirection is forbidden: ${location}`);
  if (containsCargoInstall(command) && command !== CARGO_MACHETE_INSTALL) {
    fail(`unapproved cargo install: ${location}: ${command}`);
  }
}

function validateNpmCommand(command, location, allowed) {
  if (!containsCommand(command, "npm") && !containsCommand(command, "npx")) return;
  if (!allowed.includes(command)) fail(`unapproved npm command: ${location}: ${command}`);
}

function validateSetupNodeStep(step, location) {
  if (step.uses !== SETUP_NODE) fail(`${location} must use the approved setup-node action`);
  exactKeys(step, ["uses", "with"], location);
  exactKeys(step.with, ["node-version", "package-manager-cache"], `${location}.with`);
  if (step.with["node-version"] !== "22.23.1") fail(`Node version drift: ${location}`);
  if (step.with["package-manager-cache"] !== false) fail(`${location} package-manager-cache must be false`);
}

function validateSetupJavaStep(step, location) {
  if (step.uses !== SETUP_JAVA) fail(`${location} must use the approved setup-java action`);
  exactKeys(step, ["uses", "with"], location);
  exactKeys(step.with, ["distribution", "java-version"], `${location}.with`);
  if (step.with.distribution !== "temurin") fail(`Java distribution drift: ${location}`);
  if (step.with["java-version"] !== "21.0.11+10") fail(`Java version drift: ${location}`);
}

function validateWorkflow(document, text, relative, state) {
  if (!isObject(document) || !isObject(document.jobs)) fail(`workflow jobs must be an object: ${relative}`);
  const semanticActions = [];
  for (const [jobName, job] of Object.entries(document.jobs)) {
    const location = `${relative}:jobs.${jobName}`;
    if (!isObject(job)) fail(`workflow job must be an object: ${location}`);
    if (job["runs-on"] !== "ubuntu-24.04") fail(`hosted runner drift: ${location}`);
    if (job.container !== undefined) {
      const image = typeof job.container === "string" ? job.container : job.container?.image;
      validateImage(image, `${location}.container`);
    }
    if (job.services !== undefined) {
      if (!isObject(job.services)) fail(`workflow services must be an object: ${location}`);
      for (const [serviceName, service] of Object.entries(job.services)) {
        validateImage(service?.image, `${location}.services.${serviceName}`);
      }
    }
    if (!Array.isArray(job.steps)) fail(`workflow steps must be an array: ${location}`);
    for (const [stepIndex, step] of job.steps.entries()) {
      const stepLocation = `${location}.steps[${stepIndex}]`;
      if (!isObject(step)) fail(`workflow step must be an object: ${stepLocation}`);
      if (step.uses !== undefined) {
        if (typeof step.uses !== "string") fail(`workflow action must be a string: ${stepLocation}`);
        semanticActions.push(step.uses);
        // Local composite actions (`uses: ./...`) run their own steps, which
        // this gate does not descend into — so they are forbidden outright
        // rather than silently exempted. Reintroducing one requires teaching
        // the gate to scan `.github/actions/**` with these same rules first.
        if (step.uses.startsWith("./")) {
          fail(`local composite action is unscanned and forbidden: ${stepLocation}: ${step.uses}`);
        }
        if (!APPROVED_ACTIONS.has(step.uses)) {
          fail(`floating or unapproved GitHub Action: ${stepLocation}: ${step.uses}`);
        }
        if (step.uses === SETUP_NODE && !(relative === ".github/workflows/ci.yml" && ["markdownlint", "supply-chain"].includes(jobName))) {
          fail(`setup-node is approved only in the markdownlint job and supply-chain job: ${stepLocation}`);
        }
        if (step.uses === SETUP_JAVA && !(relative === ".github/workflows/ci.yml" && jobName === "tla")) {
          fail(`setup-java is approved only in the tla job: ${stepLocation}`);
        }
      }
      if (step.run !== undefined) {
        if (typeof step.run !== "string") fail(`workflow run must be a string: ${stepLocation}`);
        validateShellCommands(step.run, stepLocation);
        if (containsCommand(step.run, "curl") || containsCommand(step.run, "wget")) {
          if (!(relative === ".github/workflows/ci.yml" && jobName === "gitleaks" && step.run.trimEnd() === GITLEAKS_EXACT_COMMAND)) {
            fail(`unapproved executable download: ${stepLocation}`);
          }
        }
        validateNpmCommand(step.run, stepLocation, ["npm ci --ignore-scripts", MARKDOWN_COMMAND]);
      }
    }
  }
  validateActionSource(text, semanticActions, relative);

  if (relative !== ".github/workflows/ci.yml") return;
  const markdown = document.jobs.markdownlint;
  if (!markdown || markdown.steps.length !== 4) fail("markdownlint job must have exactly four approved steps");
  if (markdown.steps[0].uses !== CHECKOUT) fail("markdownlint job must checkout first");
  validateSetupNodeStep(markdown.steps[1], "markdownlint setup-node step");
  if (markdown.steps[2].run !== "npm ci --ignore-scripts" || markdown.steps[3].run !== MARKDOWN_COMMAND) {
    fail("markdownlint job association/order drifted");
  }

  const supply = document.jobs["supply-chain"];
  if (!supply || supply.steps.length !== 4) fail("supply-chain job must have four approved steps");
  if (supply.steps[0].uses !== CHECKOUT) fail("supply-chain job must checkout first");
  validateSetupNodeStep(supply.steps[1], "supply-chain setup-node step");
  if (supply.steps[2].run !== "npm ci --ignore-scripts" || supply.steps[3].run !== "make supply-chain") {
    fail("supply-chain job association/order drifted");
  }

  const tla = document.jobs.tla;
  if (!tla || tla.steps.length !== 3) fail("tla job must have exactly three approved steps");
  if (tla.steps[0].uses !== CHECKOUT) fail("tla job must checkout first");
  validateSetupJavaStep(tla.steps[1], "tla setup-java step");
  if (tla.steps[2].run !== "make tla") fail("tla setup-java association/order drifted");

  const gitleaks = document.jobs.gitleaks;
  if (!gitleaks) fail("gitleaks job missing");
  const gitleaksStep = gitleaks.steps.find((step) => step.run !== undefined);
  if (!gitleaksStep || gitleaksStep.run.trimEnd() !== GITLEAKS_EXACT_COMMAND) fail("gitleaks executable identity drifted");
  if (!isObject(gitleaksStep.env) || gitleaksStep.env.GITLEAKS_VERSION !== "8.24.3") fail("gitleaks version drifted");
  state.ciSeen = true;
}

function makeTargetNames(line) {
  const match = line.match(/^([^#\s][^:]*):(?:\s|$)/u);
  return match ? match[1].trim().split(/\s+/u) : [];
}

function targetRecipes(lines, target, targetName) {
  const matches = lines.flatMap((line, index) => makeTargetNames(line).includes(targetName) ? [index] : []);
  if (matches.length === 0) fail(`Makefile target missing: ${targetName}`);
  if (matches.length > 1) fail(`duplicate Makefile target: ${targetName}`);
  const [index] = matches;
  if (lines[index] !== target) fail(`Makefile target signature drifted: ${targetName}`);
  const recipes = [];
  for (let cursor = index + 1; cursor < lines.length; cursor += 1) {
    const line = lines[cursor];
    if (line.startsWith("\t")) recipes.push(line.slice(1));
    else if (line.trim() === "" || line.trimStart().startsWith("#")) continue;
    else break;
  }
  return recipes;
}

function validateMakefile(text) {
  const lines = text.split(/\r?\n/u);
  const variables = new Map();
  for (const line of lines) {
    const target = line.match(/^([^#\s][^:]*):(?:\s|$)/u);
    if (target && /\$(?:\(|\{)/u.test(target[1])) {
      fail(`dynamic Makefile target is forbidden: ${target[1].trim()}`);
    }
    const match = line.match(/^([A-Z0-9_]+) := (.+)$/u);
    if (!match) continue;
    if (variables.has(match[1])) fail(`duplicate Makefile identity: ${match[1]}`);
    variables.set(match[1], match[2]);
  }
  const expected = new Map([
    ["TLA_TOOLS", "docs/formal/.tools/tla2tools.jar"],
    ["TLA_TOOLS_VERSION", TLA_VERSION],
    ["TLA_TOOLS_SHA256", TLA_SHA256],
    ["TLA_TOOLS_URL", TLA_URL],
  ]);
  for (const [name, value] of expected) {
    if (variables.get(name) !== value) fail(`${name} must equal ${value}`);
  }
  const downloadRecipes = targetRecipes(
    lines,
    "docs/formal/.tools/tla2tools.jar:",
    "docs/formal/.tools/tla2tools.jar",
  );
  const expectedDownload = [
    "mkdir -p docs/formal/.tools",
    "curl -fsSL $(TLA_TOOLS_URL) -o $(TLA_TOOLS).tmp",
    'echo "$(TLA_TOOLS_SHA256)  $(TLA_TOOLS).tmp" | sha256sum -c',
    "mv $(TLA_TOOLS).tmp $(TLA_TOOLS)",
  ];
  if (JSON.stringify(downloadRecipes) !== JSON.stringify(expectedDownload)) fail("TLA+ download identity/checksum sequence drifted");
  const tlaRecipes = targetRecipes(lines, "tla: $(TLA_TOOLS)", "tla");
  const expectedTla = [
    'echo "$(TLA_TOOLS_SHA256)  $(TLA_TOOLS)" | sha256sum -c',
    "cd docs/formal && java -XX:+UseParallelGC -jar .tools/tla2tools.jar -config lease_protocol.cfg lease_protocol.tla",
  ];
  if (JSON.stringify(tlaRecipes) !== JSON.stringify(expectedTla)) fail("TLA+ execution must verify the exact checksum immediately before java execution");

  const mdRecipes = targetRecipes(lines, "md:", "md");
  if (JSON.stringify(mdRecipes) !== JSON.stringify(["npm ci --ignore-scripts", MARKDOWN_COMMAND])) fail("Makefile md npm commands drifted");
  const supplyRecipes = targetRecipes(lines, "supply-chain:", "supply-chain");
  const expectedSupply = [
    "npm ci --ignore-scripts",
    "bash .github/scripts/check-supply-chain.sh",
    "bash .github/scripts/test-supply-chain.sh",
  ];
  if (JSON.stringify(supplyRecipes) !== JSON.stringify(expectedSupply)) fail("Makefile supply-chain commands drifted");

  for (const { value, number } of logicalLines(text)) {
    const line = value.trim();
    if (!line) continue;
    validateShellCommands(line, `Makefile:${number}`);
    if (containsCommand(line, "curl") || containsCommand(line, "wget")) {
      if (line !== expectedDownload[1]) fail(`unapproved executable download: Makefile:${number}`);
    }
    validateNpmCommand(line, `Makefile:${number}`, ["npm ci --ignore-scripts", MARKDOWN_COMMAND]);
  }
}

function validatePackage(packageData, lockData) {
  exactKeys(packageData, ["name", "private", "packageManager", "engines", "devDependencies"], "package.json");
  if (packageData.name !== "koine-repository-tools" || packageData.private !== true) fail("package.json repository-tool identity drifted");
  if (packageData.packageManager !== "npm@10.9.8") fail("package.json must pin packageManager to npm@10.9.8");
  exactKeys(packageData.engines, ["node"], "package.json engines");
  if (packageData.engines.node !== ">=22.23.1") fail("package.json must constrain Node to >=22.23.1");
  exactKeys(packageData.devDependencies, ["js-yaml", "markdownlint-cli2"], "package.json devDependencies");
  if (packageData.devDependencies["markdownlint-cli2"] !== "0.23.1") fail("package.json must pin markdownlint-cli2 to 0.23.1");
  if (packageData.devDependencies["js-yaml"] !== "4.3.0") fail("package.json must pin js-yaml to 4.3.0");
  if (Object.hasOwn(packageData, "scripts")) fail("package.json must not declare scripts");

  if (!isObject(lockData) || lockData.lockfileVersion !== 3 || !isObject(lockData.packages)) fail("package-lock structure drifted");
  const root = lockData.packages[""];
  if (!isObject(root)) fail("package-lock root metadata missing");
  exactKeys(root.devDependencies, ["js-yaml", "markdownlint-cli2"], "package-lock root devDependencies");
  if (root.devDependencies["js-yaml"] !== "4.3.0" || root.devDependencies["markdownlint-cli2"] !== "0.23.1") fail("package-lock root dependency pins drifted");
  if (root.engines?.node !== ">=22.23.1") fail("package-lock root Node contract drifted");

  const direct = [
    ["node_modules/js-yaml", "4.3.0", "https://registry.npmjs.org/js-yaml/-/js-yaml-4.3.0.tgz", JS_YAML_INTEGRITY],
    ["node_modules/markdownlint-cli2", "0.23.1", "https://registry.npmjs.org/markdownlint-cli2/-/markdownlint-cli2-0.23.1.tgz", MARKDOWNLINT_INTEGRITY],
  ];
  for (const [key, version, resolved, integrity] of direct) {
    const entry = lockData.packages[key];
    if (!entry || entry.version !== version) fail(`package-lock exact dependency drift: ${key}`);
    if (entry.resolved !== resolved) fail(`invalid package-lock registry source: ${key}`);
    if (entry.integrity !== integrity) fail(`invalid package-lock integrity: ${key}`);
  }
  for (const [key, entry] of Object.entries(lockData.packages)) {
    if (Object.hasOwn(entry, "scripts") || entry.hasInstallScript === true) fail(`package-lock contains install scripts: ${key}`);
    if (!key) continue;
    if (typeof entry.resolved !== "string" || !entry.resolved.startsWith("https://registry.npmjs.org/")) fail(`invalid package-lock registry source: ${key}`);
    if (typeof entry.integrity !== "string" || !/^sha512-[A-Za-z0-9+/]+={0,2}$/u.test(entry.integrity)) fail(`invalid package-lock integrity: ${key}`);
  }
}

async function validateShellScripts(root) {
  // CI checkouts only contain tracked files; these dirs are never present
  // there. `_archive` (unrelated, git-ignored legacy) is excluded too so a
  // stray local script in it can't break `make supply-chain` for a developer.
  const ignoredDirectoryNames = new Set([".git", ".worktrees", "target", "node_modules", ".superpowers", "_archive"]);
  const fixtures = path.join(root, ".github", "supply-chain-fixtures");
  const skip = (candidate, entry) => entry.isDirectory()
    && (ignoredDirectoryNames.has(entry.name) || candidate === fixtures);
  const scripts = await enumerateFiles(root, (file) => /\.(?:bash|sh|zsh)$/u.test(file), skip);
  for (const file of scripts) {
    const text = await readText(file);
    const relative = path.relative(root, file).split(path.sep).join("/");
    for (const { value, number } of logicalLines(text)) {
      const line = value.trim();
      if (!line) continue;
      validateShellCommands(line, `${relative}:${number}`);
      if (containsCommand(line, "curl") || containsCommand(line, "wget")) fail(`unapproved executable download: ${relative}:${number}`);
      validateNpmCommand(line, `${relative}:${number}`, []);
    }
  }
}

async function main() {
  const argumentsList = process.argv.slice(2);
  let root = process.cwd();
  if (argumentsList.length > 0) {
    if (argumentsList.length !== 2 || argumentsList[0] !== "--root") fail("usage: check-supply-chain.mjs [--root REPOSITORY_ROOT]");
    root = path.resolve(argumentsList[1]);
  }
  if (!versionAtLeast(process.versions.node, "22.23.1")) fail(`Node >=22.23.1 is required; found ${process.versions.node}`);

  const state = { ciSeen: false };
  const workflowRoot = path.join(root, ".github", "workflows");
  const workflowFiles = await enumerateFiles(workflowRoot, (file) => /\.ya?ml$/u.test(file));
  if (workflowFiles.length === 0) fail("filesystem scan failed: no workflows found");
  for (const file of workflowFiles) {
    const text = await readText(file);
    const relative = path.relative(root, file).split(path.sep).join("/");
    validateWorkflow(parseYaml(text, relative), text, relative, state);
  }
  if (!state.ciSeen) fail("canonical .github/workflows/ci.yml was not scanned");

  const composeText = await readText(path.join(root, "compose.yaml"));
  const compose = parseYaml(composeText, "compose.yaml");
  if (!isObject(compose) || !isObject(compose.services)) fail("compose services must be an object");
  for (const [serviceName, service] of Object.entries(compose.services)) {
    if (!isObject(service)) fail(`compose service must be an object: ${serviceName}`);
    validateImage(service.image, `compose.yaml:services.${serviceName}`);
  }

  validateMakefile(await readText(path.join(root, "Makefile")));
  const packageText = await readText(path.join(root, "package.json"));
  const lockText = await readText(path.join(root, "package-lock.json"));
  validatePackage(parseJson(packageText, "package.json"), parseJson(lockText, "package-lock.json"));
  await validateCrateLegalFiles(root);
  await validatePostgresTestcontainersHelpers(root);
  await validateShellScripts(root);
}

main().catch((error) => {
  console.error(`supply-chain check failed: ${error.message}`);
  process.exitCode = 1;
});

#!/usr/bin/env node
import { readdir, readFile, stat } from "node:fs/promises";
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
const SETUP_NODE = "actions/setup-node@a0853c24544627f65ddf259abe73b1d18a591444";
const MARKDOWN_COMMAND = 'npm exec -- markdownlint-cli2 "**/*.md" "!_archive" "!target" "!node_modules" "!docs/superpowers" "!.superpowers"';
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
    const metadata = await stat(file);
    if (!metadata.isFile()) fail(`filesystem scan failed: not a regular file: ${file}`);
    return await readFile(file, "utf8");
  } catch (error) {
    if (error instanceof Error && error.message.startsWith("filesystem scan failed")) throw error;
    fail(`filesystem scan failed for ${file}: ${error.message}`);
  }
}

async function enumerateFiles(directory, predicate) {
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

function validateImage(image, location, state) {
  if (typeof image !== "string") fail(`container image must be a string: ${location}`);
  if (location === "compose.yaml:services.postgres" && image === "postgres:17") {
    state.postgresExceptions += 1;
    return;
  }
  if (location === "compose.yaml:services.postgres" && image.startsWith("postgres:")) {
    fail(`temporary Postgres image exception drifted: ${location}: ${image}`);
  }
  if (!/@sha256:[0-9a-f]{64}$/u.test(image)) {
    fail(`container image must use a sha256 digest: ${location}: ${image}`);
  }
}

function executableShellText(source) {
  let output = "";
  let single = false;
  let double = false;
  let escaped = false;
  let substitutionDepth = 0;
  for (let index = 0; index < source.length; index += 1) {
    const char = source[index];
    const next = source[index + 1];
    if (char === "\n") {
      output += char;
      if (!single && !double) escaped = false;
      continue;
    }
    if (substitutionDepth > 0) {
      output += char;
      if (char === "(" && source[index - 1] !== "\\") substitutionDepth += 1;
      else if (char === ")" && source[index - 1] !== "\\") substitutionDepth -= 1;
      continue;
    }
    if (double) {
      if (!escaped && char === "$" && next === "(") {
        output += "$(";
        substitutionDepth = 1;
        index += 1;
      } else {
        output += " ";
        if (escaped) escaped = false;
        else if (char === "\\") escaped = true;
        else if (char === '"') double = false;
      }
      continue;
    }
    if (single) {
      output += " ";
      if (char === "'") single = false;
      continue;
    }
    if (char === '"') {
      double = true;
      output += " ";
    } else if (char === "'") {
      single = true;
      output += " ";
    } else if (char === "#" && (index === 0 || /\s/u.test(source[index - 1]))) {
      while (index < source.length && source[index] !== "\n") {
        output += " ";
        index += 1;
      }
      if (index < source.length) output += "\n";
    } else output += char;
  }
  return output;
}

function containsCommand(source, command) {
  const expression = new RegExp(`(^|[\\s;&|($])${command}(?=\\s|$)`, "mu");
  return expression.test(executableShellText(source));
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

function validateWorkflow(document, text, relative, state) {
  if (!isObject(document) || !isObject(document.jobs)) fail(`workflow jobs must be an object: ${relative}`);
  const semanticActions = [];
  for (const [jobName, job] of Object.entries(document.jobs)) {
    const location = `${relative}:jobs.${jobName}`;
    if (!isObject(job)) fail(`workflow job must be an object: ${location}`);
    if (job["runs-on"] !== "ubuntu-24.04") fail(`hosted runner drift: ${location}`);
    if (job.container !== undefined) {
      const image = typeof job.container === "string" ? job.container : job.container?.image;
      validateImage(image, `${location}.container`, state);
    }
    if (job.services !== undefined) {
      if (!isObject(job.services)) fail(`workflow services must be an object: ${location}`);
      for (const [serviceName, service] of Object.entries(job.services)) {
        validateImage(service?.image, `${location}.services.${serviceName}`, state);
      }
    }
    if (!Array.isArray(job.steps)) fail(`workflow steps must be an array: ${location}`);
    for (const [stepIndex, step] of job.steps.entries()) {
      const stepLocation = `${location}.steps[${stepIndex}]`;
      if (!isObject(step)) fail(`workflow step must be an object: ${stepLocation}`);
      if (step.uses !== undefined) {
        if (typeof step.uses !== "string") fail(`workflow action must be a string: ${stepLocation}`);
        semanticActions.push(step.uses);
        if (!step.uses.startsWith("./") && !APPROVED_ACTIONS.has(step.uses)) {
          fail(`floating or unapproved GitHub Action: ${stepLocation}: ${step.uses}`);
        }
        if (step.uses === SETUP_NODE && !(relative === ".github/workflows/ci.yml" && ["markdownlint", "supply-chain"].includes(jobName))) {
          fail(`setup-node is approved only in the markdownlint job and supply-chain job: ${stepLocation}`);
        }
      }
      if (step.run !== undefined) {
        if (typeof step.run !== "string") fail(`workflow run must be a string: ${stepLocation}`);
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

  const gitleaks = document.jobs.gitleaks;
  if (!gitleaks) fail("gitleaks job missing");
  const gitleaksStep = gitleaks.steps.find((step) => step.run !== undefined);
  if (!gitleaksStep || gitleaksStep.run.trimEnd() !== GITLEAKS_EXACT_COMMAND) fail("gitleaks executable identity drifted");
  if (!isObject(gitleaksStep.env) || gitleaksStep.env.GITLEAKS_VERSION !== "8.24.3") fail("gitleaks version drifted");
  state.ciSeen = true;
}

function targetRecipes(lines, target) {
  const index = lines.findIndex((line) => line === target);
  if (index < 0) fail(`Makefile target missing: ${target}`);
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
  const downloadRecipes = targetRecipes(lines, "$(TLA_TOOLS):");
  const expectedDownload = [
    "mkdir -p docs/formal/.tools",
    "curl -fsSL $(TLA_TOOLS_URL) -o $(TLA_TOOLS).tmp",
    'echo "$(TLA_TOOLS_SHA256)  $(TLA_TOOLS).tmp" | sha256sum -c',
    "mv $(TLA_TOOLS).tmp $(TLA_TOOLS)",
  ];
  if (JSON.stringify(downloadRecipes) !== JSON.stringify(expectedDownload)) fail("TLA+ download identity/checksum sequence drifted");
  const tlaRecipes = targetRecipes(lines, "tla: $(TLA_TOOLS)");
  const expectedTla = [
    'echo "$(TLA_TOOLS_SHA256)  $(TLA_TOOLS)" | sha256sum -c',
    "cd docs/formal && java -XX:+UseParallelGC -jar .tools/tla2tools.jar -config lease_protocol.cfg lease_protocol.tla",
  ];
  if (JSON.stringify(tlaRecipes) !== JSON.stringify(expectedTla)) fail("TLA+ execution must verify the exact checksum immediately before java execution");

  const mdRecipes = targetRecipes(lines, "md:");
  if (JSON.stringify(mdRecipes) !== JSON.stringify(["npm ci --ignore-scripts", MARKDOWN_COMMAND])) fail("Makefile md npm commands drifted");
  const supplyRecipes = targetRecipes(lines, "supply-chain:");
  const expectedSupply = [
    "npm ci --ignore-scripts",
    "bash .github/scripts/check-supply-chain.sh",
    "bash .github/scripts/test-supply-chain.sh",
  ];
  if (JSON.stringify(supplyRecipes) !== JSON.stringify(expectedSupply)) fail("Makefile supply-chain commands drifted");

  for (const [index, rawLine] of lines.entries()) {
    const line = rawLine.trim();
    if (!line) continue;
    if (containsCommand(line, "curl") || containsCommand(line, "wget")) {
      if (line !== expectedDownload[1]) fail(`unapproved executable download: Makefile:${index + 1}`);
    }
    validateNpmCommand(line, `Makefile:${index + 1}`, ["npm ci --ignore-scripts", MARKDOWN_COMMAND]);
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
  const directory = path.join(root, ".github", "scripts");
  const scripts = await enumerateFiles(directory, (file) => file.endsWith(".sh"));
  for (const file of scripts) {
    const text = await readText(file);
    const relative = path.relative(root, file);
    for (const [index, rawLine] of text.split(/\r?\n/u).entries()) {
      const line = rawLine.trim();
      if (!line) continue;
      if (containsCommand(line, "curl") || containsCommand(line, "wget")) fail(`unapproved executable download: ${relative}:${index + 1}`);
      validateNpmCommand(line, `${relative}:${index + 1}`, []);
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

  const state = { ciSeen: false, postgresExceptions: 0 };
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
    validateImage(service.image, `compose.yaml:services.${serviceName}`, state);
  }
  if (state.postgresExceptions !== 1) fail("compose.yaml must contain the one temporary postgres:17 exception owned by Operational Task 4");

  validateMakefile(await readText(path.join(root, "Makefile")));
  const packageText = await readText(path.join(root, "package.json"));
  const lockText = await readText(path.join(root, "package-lock.json"));
  validatePackage(parseJson(packageText, "package.json"), parseJson(lockText, "package-lock.json"));
  await validateShellScripts(root);
}

main().catch((error) => {
  console.error(`supply-chain check failed: ${error.message}`);
  process.exitCode = 1;
});

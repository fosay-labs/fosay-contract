// Verify that every backtick-quoted `module::test_name` in SECURITY.md
// exists in the output of `cargo test --workspace -- --list`.
//
//   node scripts/check-security-tests.mjs
import fs from 'node:fs';
import path from 'node:path';
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const REPO = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const securityPath = process.env.SECURITY_MD_PATH ?? path.join(REPO, 'SECURITY.md');
const markdown = fs.readFileSync(securityPath, 'utf8');

const startHeading = '### Invariants → tests';
const endHeading = '## Reporting a vulnerability';
const startIdx = markdown.indexOf(startHeading);
if (startIdx === -1) {
  console.error(`ERROR: Could not find "${startHeading}" section in ${securityPath}`);
  process.exit(1);
}
const endIdx = markdown.indexOf(endHeading, startIdx);
const tableSection = endIdx === -1 ? markdown.slice(startIdx) : markdown.slice(startIdx, endIdx);

const referenced = new Set();
for (const match of tableSection.matchAll(/`([a-z0-9_]+::[a-z0-9_]+)`/g)) {
  referenced.add(match[1]);
}

if (referenced.size === 0) {
  console.error(`ERROR: No backtick-quoted module::test_name entries found in ${securityPath}`);
  process.exit(1);
}

const listOutput =
  process.env.CARGO_TEST_LIST_OUTPUT ??
  execFileSync('cargo', ['test', '--workspace', '--', '--list'], {
    cwd: REPO,
    encoding: 'utf8',
    stdio: ['ignore', 'pipe', 'inherit'],
  });

const discovered = new Set();
for (const rawLine of listOutput.split(/\r?\n/)) {
  const line = rawLine.trim();
  if (!line.endsWith(': test')) continue;
  const fullPath = line.slice(0, -': test'.length).trim();
  discovered.add(fullPath);
  if (fullPath.startsWith('test::')) {
    discovered.add(fullPath.slice('test::'.length));
  }
}

const missing = [];
for (const ref of referenced) {
  const found =
    discovered.has(ref) ||
    discovered.has(`test::${ref}`) ||
    Array.from(discovered).some((name) => name.endsWith(`::${ref}`));
  if (!found) {
    missing.push(ref);
  }
}

if (missing.length > 0) {
  console.error(
    `ERROR: ${missing.length} test(s) referenced in SECURITY.md were not found in cargo test --workspace -- --list:`,
  );
  for (const name of missing) {
    console.error(`  - ${name}`);
  }
  process.exit(1);
}

console.log(`Verified ${referenced.size} SECURITY.md test references against cargo test --workspace -- --list.`);

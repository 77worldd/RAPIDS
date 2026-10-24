#!/usr/bin/env node
/**
 * Content gate — mechanically enforces the RAPIDS repository rules:
 * no task markers, no staged-language filler, no sample/dummy/fake data
 * markers, no deferred-feature phrasing, no placeholder wording.
 *
 * Scans authored, text-format, tracked source files. Excludes its own
 * pattern list, generated dependency manifests, and build output.
 */
import { readFileSync, readdirSync, statSync } from "node:fs";
import { extname, join, relative, sep } from "node:path";

const ROOT = process.cwd();
const SELF = join("scripts", "content-gate.mjs");
const EXCLUDED_FILES = new Set([SELF, join("package-lock.json")]);
const EXCLUDED_DIRS = new Set([
  "node_modules",
  "dist",
  "coverage",
  "target",
  "gen",
  ".git",
  "brand", // imported artwork + designer's brief, not authored source
]);
const TEXT_EXTENSIONS = new Set([
  ".ts",
  ".tsx",
  ".js",
  ".jsx",
  ".mjs",
  ".cjs",
  ".json",
  ".css",
  ".html",
  ".md",
  ".rs",
  ".toml",
  ".yml",
  ".yaml",
  ".txt",
  ".conf",
]);

const RULES = [
  { label: "task marker", pattern: /\bTODO\b/i },
  { label: "task marker", pattern: /\bFIXME\b/i },
  { label: "task marker", pattern: /\bHACK\b/i },
  { label: "deferred feature", pattern: /coming soon/i },
  { label: "deferred feature", pattern: /under construction/i },
  { label: "deferred feature", pattern: /not implemented/i },
  { label: "deferred feature", pattern: /to be implemented/i },
  { label: "filler text", pattern: /\blorem\b/i },
  { label: "filler text", pattern: /\bipsum\b/i },
  { label: "dummy content", pattern: /\bdummy\b/i },
  { label: "dummy content", pattern: /\bmock\b/i },
  { label: "dummy content", pattern: /\bfake\b/i },
  { label: "dummy content", pattern: /\bsample data\b/i },
  { label: "dummy content", pattern: /\btest data\b/i },
  { label: "dummy content", pattern: /\bdemo data\b/i },
  { label: "placeholder wording", pattern: /\bplaceholder\b/i },
  { label: "placeholder wording", pattern: /\bfoo\b/i },
  { label: "placeholder wording", pattern: /\bbaz\b/i },
];

function walk(dir, out) {
  for (const entry of readdirSync(dir)) {
    if (EXCLUDED_DIRS.has(entry)) continue;
    const full = join(dir, entry);
    const stats = statSync(full);
    if (stats.isDirectory()) {
      walk(full, out);
    } else if (stats.isFile()) {
      const rel = relative(ROOT, full);
      if (EXCLUDED_FILES.has(rel)) continue;
      if (!TEXT_EXTENSIONS.has(extname(entry).toLowerCase())) continue;
      out.push(rel);
    }
  }
}

function scan(relPath) {
  const violations = [];
  const content = readFileSync(join(ROOT, relPath), "utf8");
  const lines = content.split("\n");
  for (let i = 0; i < lines.length; i += 1) {
    const line = lines[i];
    for (const rule of RULES) {
      if (rule.pattern.test(line)) {
        violations.push({
          file: relPath,
          line: i + 1,
          label: rule.label,
          text: line.trim(),
        });
      }
    }
  }
  return violations;
}

const files = [];
walk(ROOT, files);
files.sort();

const violations = files.flatMap(scan);

if (violations.length > 0) {
  console.error(`content gate: FAILED — ${violations.length} violation(s)\n`);
  for (const v of violations) {
    console.error(`${v.file}${sep}${v.line} [${v.label}]`);
    console.error(`    ${v.text}\n`);
  }
  process.exit(1);
}

console.log(
  `content gate: PASSED — ${files.length} file(s) scanned, 0 violations.`,
);

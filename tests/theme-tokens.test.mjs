import assert from "node:assert/strict";
import { readFileSync, readdirSync, statSync } from "node:fs";
import { join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("..", import.meta.url));
const appCss = readFileSync(join(root, "src", "index.css"), "utf8");
const extensionCss = readFileSync(join(root, "extension", "theme.css"), "utf8");

const PALETTE_TOKENS = [
  "canvas",
  "panel",
  "elevated",
  "line",
  "line-strong",
  "ink",
  "ink-soft",
  "muted",
  "faint",
];

function blockAfter(css, marker) {
  const start = css.indexOf(marker);
  assert.notEqual(start, -1, `missing block: ${marker}`);
  const open = css.indexOf("{", start);
  const close = css.indexOf("}", open);
  assert.notEqual(open, -1, `missing opening brace after ${marker}`);
  assert.notEqual(close, -1, `missing closing brace after ${marker}`);
  return css.slice(open + 1, close);
}

function palette(block) {
  const values = {};
  for (const match of block.matchAll(/--([a-z-]+):\s*(#[0-9a-fA-F]{6})\s*;/g)) {
    values[match[1]] = match[2].toLowerCase();
  }
  return values;
}

function selectPalette(block) {
  const all = palette(block);
  const selected = {};
  for (const token of PALETTE_TOKENS) {
    assert.equal(
      Object.prototype.hasOwnProperty.call(all, token),
      true,
      `missing --${token}`,
    );
    selected[token] = all[token];
  }
  return selected;
}

function filesUnder(directory, extensions) {
  const found = [];
  for (const entry of readdirSync(directory)) {
    const path = join(directory, entry);
    if (statSync(path).isDirectory()) {
      found.push(...filesUnder(path, extensions));
    } else if (extensions.some((extension) => entry.endsWith(extension))) {
      found.push(path);
    }
  }
  return found;
}

test("app and extension share identical dark and light palettes", () => {
  const appDark = selectPalette(blockAfter(appCss, ":root {"));
  const extensionDark = selectPalette(blockAfter(extensionCss, ":root {"));
  assert.deepEqual(appDark, extensionDark);

  const appLight = selectPalette(
    blockAfter(appCss, ':root[data-theme="light"]'),
  );
  const extensionLight = selectPalette(
    blockAfter(extensionCss, ':root[data-theme="light"]'),
  );
  assert.deepEqual(appLight, extensionLight);

  const appSystem = selectPalette(
    blockAfter(appCss, ":root:not([data-theme])"),
  );
  const extensionSystem = selectPalette(
    blockAfter(extensionCss, ":root:not([data-theme])"),
  );
  assert.deepEqual(appSystem, extensionSystem);
  assert.deepEqual(appSystem, appLight);
});

test("both palettes follow the operating system preference", () => {
  assert.match(appCss, /@media\s*\(prefers-color-scheme:\s*light\)/);
  assert.match(extensionCss, /@media\s*\(prefers-color-scheme:\s*light\)/);
  assert.match(appCss, /color-scheme:\s*light/);
  assert.match(extensionCss, /color-scheme:\s*light/);
});

test("no hue survives in app or extension source", () => {
  const files = [
    ...filesUnder(join(root, "src"), [".ts", ".tsx", ".css"]),
    ...filesUnder(join(root, "extension"), [".js", ".css", ".html", ".json"]),
  ];
  const bannedWords = /\b(emerald|green|teal|lime)\b/i;
  const bannedHex = /#(10b981|059669|34d399|6ee7b7|a7f3d0|022c22|03251a)/i;
  const bannedRgb = /rgba?\(\s*16\s*[, ]\s*185\s*[, ]\s*129/i;
  const bannedUtility =
    /\b(?:bg|text|border|ring|from|via|to|divide|outline|stroke|fill|decoration|caret|accent|placeholder)-(?:red|rose|pink|orange|amber|yellow|lime|green|emerald|teal|cyan|sky|blue|indigo|violet|purple|fuchsia)-\d{2,3}\b/;
  const violations = [];
  for (const file of files) {
    const source = readFileSync(file, "utf8");
    for (const check of [bannedWords, bannedHex, bannedRgb, bannedUtility]) {
      const match = check.exec(source);
      if (match !== null) {
        violations.push(`${file.slice(root.length)}: ${match[0]}`);
      }
    }
  }
  assert.deepEqual(violations, []);
});

test("every extension CSS variable resolves in the shared palette", () => {
  const sources = ["popup.css", "options.css"].map((name) =>
    readFileSync(join(root, "extension", name), "utf8"),
  );
  const defined = new Set(
    [...extensionCss.matchAll(/--([a-z-]+)\s*:/g)].map((match) => match[1]),
  );
  const missing = [];
  for (const source of sources) {
    for (const match of source.matchAll(/var\(--([a-z-]+)\)/g)) {
      if (!defined.has(match[1])) missing.push(match[1]);
    }
  }
  assert.deepEqual([...new Set(missing)], []);
});

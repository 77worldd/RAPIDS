import assert from "node:assert/strict";
import { readFileSync, readdirSync, statSync } from "node:fs";
import { join, extname } from "node:path";
import { test } from "node:test";

const root = process.cwd();

function filesUnder(directory, extensions) {
  const found = [];
  for (const entry of readdirSync(directory)) {
    const path = join(directory, entry);
    if (statSync(path).isDirectory()) {
      found.push(...filesUnder(path, extensions));
      continue;
    }
    if (extensions.includes(extname(path))) found.push(path);
  }
  return found;
}

const ACCENTS = {
  accent: "#1fb9df",
  "accent-bright": "#62e4f4",
  "accent-deep": "#0c7ca4",
  "accent-warm": "#f97316",
  "accent-amber": "#fbae3c",
};

const SURFACES = {
  canvas: "#0b1017",
  panel: "#101823",
  elevated: "#15202f",
  sidebar: "#0d141d",
  line: "#223449",
  muted: "#8a94a3",
  high: "#f4f9fc",
};

test("the theme exposes the locked brand accents and ink-tinted surfaces", () => {
  const css = readFileSync(join(root, "src", "index.css"), "utf8");
  for (const [token, hex] of Object.entries({ ...ACCENTS, ...SURFACES })) {
    assert.match(
      css,
      new RegExp(`--color-${token}:\\s*${hex};`, "i"),
      `${token} must stay ${hex}`,
    );
  }
  assert.match(css, /::selection\s*\{[^}]*rgb\(31 185 223 \/ 0\.35\)/s);
  assert.match(css, /radial-gradient\([^)]*rgb\(31 185 223 \/ 0\.07\)/s);
});

test("no stock green accent survives in the app source", () => {
  const bannedUtility =
    /\b(?:emerald|green|teal|lime)-(?:\d{2,3}|\[[^\]]+\])/i;
  const bannedHex = /#(?:10b981|34d399|6ee7b7|a7f3d0|059669|022c22|03251a)\b/i;
  const bannedRgb = /rgba?\(\s*16\s*[, ]\s*185\s*[, ]\s*129/i;
  const violations = [];
  for (const file of filesUnder(join(root, "src"), [".ts", ".tsx", ".css"])) {
    const source = readFileSync(file, "utf8");
    for (const check of [bannedUtility, bannedHex, bannedRgb]) {
      const match = check.exec(source);
      if (match !== null) violations.push(`${file}: ${match[0]}`);
    }
  }
  assert.deepEqual(violations, []);
});

test("the extension carries the brand accent instead of green", () => {
  const background = readFileSync(
    join(root, "extension", "background.js"),
    "utf8",
  );
  assert.match(background, /setBadgeBackgroundColor\(\{\s*color:\s*"#1fb9df"\s*\}\)/i);
  for (const name of ["options.css", "popup.css"]) {
    const css = readFileSync(join(root, "extension", name), "utf8");
    assert.match(css, /--accent:\s*#1fb9df;/i, `${name} needs the accent token`);
    assert.match(css, /--accent-bright:\s*#62e4f4;/i);
    assert.match(css, /--accent-deep:\s*#0c7ca4;/i);
    assert.doesNotMatch(css, /emerald|#059669|#10b981/i);
    assert.doesNotMatch(css, /rgba?\(\s*16\s*,\s*185\s*,\s*129/i);
  }
});

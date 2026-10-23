import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import vm from "node:vm";

const source = readFileSync(
  new URL("../extension/handoff.js", import.meta.url),
  "utf8",
);
const context = vm.createContext({
  URL,
  URLSearchParams,
  encodeURIComponent,
  decodeURIComponent,
});
vm.runInContext(source, context, { filename: "handoff.js" });
const handoff = context.RapidsHandoff;

function parameters(raw) {
  const url = new URL(raw);
  assert.equal(url.protocol, "rapids:");
  assert.equal(url.hostname, "add");
  return url.searchParams;
}

test("a minimal candidate builds a canonical protocol v1 link", () => {
  const raw = handoff.buildAddUrl({
    url: "https://example.com/file.mp4",
    name: null,
    kind: "file",
    pageUrl: null,
    size: null,
  });
  const query = parameters(raw);
  assert.equal(query.get("u"), "https://example.com/file.mp4");
  assert.equal(query.get("n"), "file.mp4");
  assert.equal(query.get("t"), "file");
  assert.equal(query.get("v"), "1");
  assert.equal(query.get("p"), null);
  assert.equal(query.get("sz"), null);
});

test("all supported hints are encoded without loss", () => {
  const raw = handoff.buildAddUrl({
    url: "https://cdn.example.com/My%20Movie%20%E2%9C%93.mkv",
    name: "My Movie ✓.mkv",
    kind: "video",
    pageUrl: "https://example.com/watch?x=1",
    size: 123456,
  });
  const query = parameters(raw);
  assert.equal(
    query.get("u"),
    "https://cdn.example.com/My%20Movie%20%E2%9C%93.mkv",
  );
  assert.equal(query.get("n"), "My Movie ✓.mkv");
  assert.equal(query.get("t"), "video");
  assert.equal(query.get("p"), "https://example.com/watch?x=1");
  assert.equal(query.get("sz"), "123456");
  assert.equal(raw.length <= handoff.MAX_URL_LENGTH, true);
});

test("unsafe names are stripped before encoding", () => {
  const raw = handoff.buildAddUrl({
    url: "https://example.com/file.mkv",
    name: "../evil\\clip\n.mkv",
    kind: "video",
    pageUrl: null,
    size: null,
  });
  const name = parameters(raw).get("n");
  assert.equal(name, "..evilclip.mkv");
  assert.equal(
    [...name].some((character) => {
      const code = character.codePointAt(0) ?? 0;
      return (
        character === "/" || character === "\\" || code < 0x20 || code === 0x7f
      );
    }),
    false,
  );
});

test("non-HTTP downloads and overlong handoffs are refused", () => {
  assert.throws(
    () => handoff.buildAddUrl({ url: "file:///C:/secret.mkv", kind: "video" }),
    /HTTP or HTTPS/,
  );
  assert.throws(
    () => handoff.buildAddUrl({ url: "javascript:alert(1)", kind: "file" }),
    /HTTP or HTTPS/,
  );
  const longUrl = "https://e.co/" + "a".repeat(4096 - "https://e.co/".length);
  assert.equal(longUrl.length, 4096);
  assert.throws(
    () => handoff.buildAddUrl({ url: longUrl, kind: "file" }),
    /too long/,
  );
});

test("Content-Disposition names decode both filename forms", () => {
  assert.equal(
    handoff.fileNameFromDisposition(
      'attachment; filename="Sintel.2010.1080p.mkv"',
    ),
    "Sintel.2010.1080p.mkv",
  );
  assert.equal(
    handoff.fileNameFromDisposition(
      "attachment; filename*=UTF-8''caf%C3%A9%20%E2%9C%93.mkv",
    ),
    "café ✓.mkv",
  );
  assert.equal(
    handoff.fileNameFromDisposition('attachment; filename="..\\..\\evil.mkv"'),
    "....evil.mkv",
  );
  assert.equal(handoff.fileNameFromDisposition(""), null);
});

test("URL names and extensions follow the desktop rules", () => {
  assert.equal(
    handoff.fileNameFromUrl("https://example.com/My%20Movie%20%E2%9C%93.mkv"),
    "My Movie ✓.mkv",
  );
  assert.equal(
    handoff.extensionOf("https://example.com/archive.ZIP?x=1"),
    "zip",
  );
  assert.equal(handoff.isDirectCandidateUrl("https://example.com/a.mkv"), true);
  assert.equal(handoff.isDirectCandidateUrl("https://example.com/a.pdf"), true);
  assert.equal(
    handoff.isDirectCandidateUrl("https://example.com/page.html"),
    false,
  );
  assert.equal(
    handoff.isDirectCandidateUrl("https://example.com/live.m3u8"),
    false,
  );
  assert.equal(handoff.kindFrom("video/mp4", "https://example.com/x"), "video");
  assert.equal(
    handoff.kindFrom("audio/mpeg", "https://example.com/x"),
    "audio",
  );
  assert.equal(handoff.kindFrom("", "https://example.com/x.zip"), "file");
});

test("dedupe keys ignore only fragments", () => {
  const first = handoff.dedupeKey("https://example.com/file.mkv#one");
  const second = handoff.dedupeKey("https://example.com/file.mkv#two");
  const query = handoff.dedupeKey("https://example.com/file.mkv?x=1");
  assert.equal(first, second);
  assert.notEqual(first, query);
  assert.equal(handoff.dedupeKey("data:text/plain,x"), null);
});

test("the synthetic anchor is only used for a valid rapids add link", () => {
  const clicks = [];
  let removed = 0;
  const anchor = {
    href: "",
    rel: "",
    style: {},
    click() {
      clicks.push(this.href);
    },
    remove() {
      removed += 1;
    },
  };
  context.document = {
    body: {
      appendChild(node) {
        assert.equal(node, anchor);
      },
    },
    documentElement: null,
    createElement(tag) {
      assert.equal(tag, "a");
      return anchor;
    },
  };

  const raw = handoff.buildAddUrl({
    url: "https://example.com/file.mkv",
    kind: "video",
    pageUrl: "https://example.com/page",
    size: 10,
  });
  assert.equal(handoff.isRapidsAddUrl(raw), true);
  assert.equal(handoff.triggerAnchor(raw), true);
  assert.deepEqual(clicks, [raw]);
  assert.equal(removed, 1);
  assert.equal(handoff.triggerAnchor("https://example.com/file.mkv"), false);
  assert.equal(
    handoff.triggerAnchor(`rapids://add?u=${"a".repeat(4096)}`),
    false,
  );
});

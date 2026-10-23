import assert from "node:assert/strict";
import { test } from "node:test";
import {
  parseDeepLinkRequest,
  parsePendingDeepLink,
} from "../src/lib/deeplink.ts";

const ADD_EVENT = {
  url: "https://cdn.example.com/movie.mkv",
  name: "Movie.mkv",
  kind: "video",
  sourcePage: "https://example.com/watch",
  size: 123456,
};

test("a complete host payload is preserved", () => {
  assert.deepEqual(parseDeepLinkRequest(ADD_EVENT), {
    url: "https://cdn.example.com/movie.mkv",
    name: "Movie.mkv",
    kind: "video",
    sourcePage: "https://example.com/watch",
    size: 123456,
  });
});

test("the download URL must be HTTP or HTTPS", () => {
  assert.equal(
    parseDeepLinkRequest({ ...ADD_EVENT, url: "file:///C:/secret" }),
    null,
  );
  assert.equal(
    parseDeepLinkRequest({ ...ADD_EVENT, url: "javascript:alert(1)" }),
    null,
  );
  assert.equal(
    parseDeepLinkRequest({
      ...ADD_EVENT,
      url: `https://example.com/${"a".repeat(4096)}`,
    }),
    null,
  );
});

test("malformed optional hints never block the download", () => {
  assert.deepEqual(
    parseDeepLinkRequest({
      ...ADD_EVENT,
      name: "../evil\nname.mkv",
      kind: "executable",
      sourcePage: "chrome://settings",
      size: Number.MAX_VALUE,
    }),
    {
      url: "https://cdn.example.com/movie.mkv",
      name: null,
      kind: "file",
      sourcePage: null,
      size: null,
    },
  );
});

test("a missing hint set becomes safe defaults", () => {
  assert.deepEqual(
    parseDeepLinkRequest({ url: "https://example.com/file.zip" }),
    {
      url: "https://example.com/file.zip",
      name: null,
      kind: "file",
      sourcePage: null,
      size: null,
    },
  );
});

test("the pending queue accepts add and error entries", () => {
  assert.deepEqual(parsePendingDeepLink({ kind: "add", request: ADD_EVENT }), {
    kind: "add",
    request: {
      url: "https://cdn.example.com/movie.mkv",
      name: "Movie.mkv",
      kind: "video",
      sourcePage: "https://example.com/watch",
      size: 123456,
    },
  });
  assert.deepEqual(
    parsePendingDeepLink({ kind: "error", reason: "Unsupported action." }),
    {
      kind: "error",
      reason: "Unsupported action.",
    },
  );
});

test("unknown, malformed, and empty pending entries are ignored", () => {
  assert.equal(
    parsePendingDeepLink({ kind: "save", request: ADD_EVENT }),
    null,
  );
  assert.equal(
    parsePendingDeepLink({ kind: "add", request: { url: "ftp://x/y" } }),
    null,
  );
  assert.equal(parsePendingDeepLink({ kind: "error", reason: "   " }), null);
  assert.equal(parsePendingDeepLink(null), null);
});

test("an event payload is not mistaken for a pending queue entry", () => {
  assert.equal(parsePendingDeepLink(ADD_EVENT), null);
});

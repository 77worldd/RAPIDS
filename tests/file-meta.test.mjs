import assert from "node:assert/strict";
import { test } from "node:test";
import {
  inspectUrl,
  isDirectFile,
  normalizeClipboardUrl,
  suggestedCategory,
  urlKey,
} from "../src/lib/fileMeta.ts";

test("a direct file link reveals its name, type, category, and host", () => {
  const info = inspectUrl("https://cdn.rapids.invalid/media/archive.zip?token=1#part");
  assert.equal(info.valid, true);
  assert.equal(info.fileName, "archive.zip");
  assert.equal(info.extension, "zip");
  assert.equal(info.typeLabel, "ZIP archive");
  assert.equal(info.category, "archives");
  assert.equal(info.kind, "archive");
  assert.equal(info.host, "cdn.rapids.invalid");
  assert.equal(info.isStreaming, false);
  assert.equal(info.isPageLike, false);
  assert.equal(isDirectFile(info), true);
  assert.equal(suggestedCategory(info), "archives");
});

test("percent-encoded names are decoded before they reach the queue", () => {
  const info = inspectUrl("https://rapids.invalid/files/My%20Document.pdf");
  assert.equal(info.fileName, "My Document.pdf");
  assert.equal(info.extension, "pdf");
  assert.equal(info.category, "documents");
});

test("streaming manifests and pages are reported instead of treated as files", () => {
  const stream = inspectUrl("https://rapids.invalid/live/index.m3u8");
  assert.equal(stream.isStreaming, true);
  assert.equal(isDirectFile(stream), false);

  const manifest = inspectUrl("https://rapids.invalid/live/index.mpd");
  assert.equal(manifest.isStreaming, true);
  assert.equal(isDirectFile(manifest), false);

  const page = inspectUrl("https://rapids.invalid/watch/movie.html");
  assert.equal(page.isPageLike, true);
  assert.equal(isDirectFile(page), false);
});

test("a URL without a final file name is not a direct file", () => {
  const folder = inspectUrl("https://rapids.invalid/releases/latest/");
  assert.equal(folder.valid, true);
  assert.equal(folder.fileName, null);
  assert.equal(isDirectFile(folder), false);

  const hostOnly = inspectUrl("https://rapids.invalid");
  assert.equal(hostOnly.fileName, null);
  assert.equal(hostOnly.extension, null);
});

test("only credential-free HTTP and HTTPS links are accepted", () => {
  assert.equal(inspectUrl("ftp://rapids.invalid/file.zip").valid, false);
  assert.equal(
    inspectUrl("https://user:secret@rapids.invalid/file.zip").valid,
    false,
  );
  assert.equal(inspectUrl("not a link").valid, false);
});

test("duplicate keys ignore a fragment and casing while keeping query data", () => {
  assert.equal(
    urlKey("HTTPS://RAPIDS.INVALID/File.ZIP?token=A#one"),
    urlKey("https://rapids.invalid/file.zip?token=A#two"),
  );
  assert.notEqual(
    urlKey("https://rapids.invalid/file.zip?token=A"),
    urlKey("https://rapids.invalid/file.zip?token=B"),
  );
});

test("clipboard normalization takes the first non-empty URL line", () => {
  assert.equal(
    normalizeClipboardUrl("\r\n  https://rapids.invalid/file.iso  \r\nsecond"),
    "https://rapids.invalid/file.iso",
  );
  assert.equal(normalizeClipboardUrl("no link here"), null);
  assert.equal(normalizeClipboardUrl("   "), null);
});

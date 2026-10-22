import assert from "node:assert/strict";
import { test } from "node:test";
import { createAddDraft, toAddRequest } from "../src/lib/addDraft.ts";

function draft(changes = {}) {
  return {
    ...createAddDraft(),
    url: "https://rapids.invalid/archive.zip",
    ...changes,
  };
}

test("a basic request uses the host's real defaults", () => {
  assert.deepEqual(toAddRequest(draft()), {
    url: "https://rapids.invalid/archive.zip",
    category: "other",
    priority: 0,
  });
  assert.equal(
    "destDir" in toAddRequest(draft({ destDir: "  " })),
    false,
    "an omitted folder lets the adapter choose the configured or Downloads path",
  );
});

test("typed transfer options keep their units and names at the wire boundary", () => {
  assert.deepEqual(
    toAddRequest(
      draft({
        destDir: "C:\\Downloads",
        category: "apps",
        priority: "-2",
        connections: "4",
        speedLimitBps: "4096",
        expectedSha256: "A".repeat(64),
      }),
    ),
    {
      url: "https://rapids.invalid/archive.zip",
      category: "apps",
      destDir: "C:\\Downloads",
      priority: -2,
      connections: 4,
      speedLimitBps: 4096,
      expectedSha256: "a".repeat(64),
    },
  );
  assert.equal(
    toAddRequest(draft({ destDir: "\\\\server\\share" })).destDir,
    "\\\\server\\share",
  );
});

test("a malformed or credential-bearing URL is refused before enqueue", () => {
  assert.throws(() => toAddRequest(draft({ url: "" })), /complete HTTP/);
  assert.throws(() => toAddRequest(draft({ url: "ftp://rapids.invalid/file" })), /HTTP and HTTPS/);
  assert.throws(
    () => toAddRequest(draft({ url: "https://user:x@rapids.invalid/file" })),
    /username or password/,
  );
});

test("a relative destination cannot silently write into the app's working directory", () => {
  assert.throws(() => toAddRequest(draft({ destDir: "downloads" })), /absolute Windows path/);
});

test("numbers must be integral and respect the per-task bounds", () => {
  assert.throws(() => toAddRequest(draft({ priority: "1.5" })), /Priority must be a whole number/);
  assert.throws(() => toAddRequest(draft({ connections: "0" })), /between 1 and 32/);
  assert.throws(() => toAddRequest(draft({ connections: "33" })), /between 1 and 32/);
  assert.throws(() => toAddRequest(draft({ speedLimitBps: "0" })), /at least 1 byte/);
  assert.throws(() => toAddRequest(draft({ speedLimitBps: "1e30" })), /whole number/);
});

test("an expected hash must contain 64 hex digits", () => {
  assert.throws(() => toAddRequest(draft({ expectedSha256: "a".repeat(63) })), /64 hexadecimal/);
  assert.throws(() => toAddRequest(draft({ expectedSha256: "z".repeat(64) })), /64 hexadecimal/);
});

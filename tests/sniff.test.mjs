import assert from "node:assert/strict";
import { test } from "node:test";
import {
  createSniffInbox,
  dismissSniffInbox,
  receiveSniffCandidate,
  sniffCandidateLabel,
  toInspectorUrl,
  toSniffCandidate,
} from "../src/lib/sniff.ts";

const PAGE = "http://127.0.0.1:53001/page.html";
const CLIP = "http://127.0.0.1:53001/clip.mp4?signature=Q%2B9";
const TRAILER = "http://127.0.0.1:53001/trailer.mp4";

function candidate(changes = {}) {
  return { session: 7, pageUrl: PAGE, url: CLIP, source: "video", ...changes };
}

test("opening the inspector requires a bounded credential-free HTTP URL", () => {
  assert.equal(
    toInspectorUrl("HTTPS://MEDIA.EXAMPLE:443/film.mp4?signature=Q%2B9"),
    "https://media.example/film.mp4?signature=Q%2B9",
  );
  for (const input of [
    "",
    "not a URL",
    "ftp://media.example/film.mp4",
    "file:///film.mp4",
    "https://user:secret@media.example/film.mp4",
    " https://media.example/film.mp4",
    `https://media.example/${"a".repeat(2050)}`,
  ]) {
    assert.throws(() => toInspectorUrl(input), /HTTP or HTTPS page URL/);
  }
});

test("the typed boundary accepts native video, link, and navigation sources", () => {
  for (const source of ["video", "videoSource", "link"]) {
    assert.deepEqual(
      toSniffCandidate(candidate({ source })),
      candidate({ source }),
    );
  }
  const direct = candidate({
    pageUrl: TRAILER,
    url: TRAILER,
    source: "navigation",
  });
  assert.deepEqual(toSniffCandidate(direct), direct);
  const dynamic = candidate({
    url: "https://media.example/get?id=42",
    source: "videoSource",
  });
  assert.deepEqual(toSniffCandidate(dynamic), dynamic);
});

test("invalid or nonconforming native events cannot enter the inbox", () => {
  const invalid = [
    null,
    [],
    {},
    candidate({ session: 0 }),
    candidate({ session: 1.5 }),
    candidate({ session: Number.MAX_SAFE_INTEGER + 1 }),
    candidate({ source: "xhr" }),
    candidate({ url: "file:///clip.mp4" }),
    candidate({ url: "https://user:secret@media.example/clip.mp4" }),
    candidate({ url: "http://127.0.0.1:53001/stream.m3u8" }),
    candidate({ url: "http://127.0.0.1:53001/stream.mpd" }),
    candidate({ url: "http://127.0.0.1:53001/get?id=42", source: "link" }),
    candidate({ url: `${TRAILER}#track` }),
    candidate({ pageUrl: "https://user:secret@media.example/page.html" }),
    candidate({ source: "navigation" }),
    { ...candidate(), unexpected: "unused" },
  ];
  for (const payload of invalid) {
    assert.equal(toSniffCandidate(payload), null);
  }
});

test("one page retains distinct media but deduplicates URLs and caps suggestions", () => {
  const first = toSniffCandidate(candidate());
  const linked = toSniffCandidate(candidate({ url: TRAILER, source: "link" }));
  assert.ok(first);
  assert.ok(linked);

  let inbox = receiveSniffCandidate(createSniffInbox(), first);
  assert.equal(
    receiveSniffCandidate(inbox, candidate({ source: "link" })),
    inbox,
  );
  inbox = receiveSniffCandidate(inbox, linked);
  assert.deepEqual(
    inbox.candidates.map((item) => item.url),
    [CLIP, TRAILER],
  );

  for (let index = 0; index < 30; index += 1) {
    const next = toSniffCandidate(
      candidate({
        url: `http://127.0.0.1:53001/${index}.mp4`,
        source: "link",
      }),
    );
    assert.ok(next);
    inbox = receiveSniffCandidate(inbox, next);
  }
  assert.equal(inbox.candidates.length, 16);
});

test("dismissal mutes this page, while a later page resets the inbox", () => {
  const first = toSniffCandidate(candidate());
  const linked = toSniffCandidate(candidate({ url: TRAILER, source: "link" }));
  assert.ok(first);
  assert.ok(linked);

  let inbox = receiveSniffCandidate(createSniffInbox(), first);
  inbox = dismissSniffInbox(inbox);
  assert.deepEqual(inbox.candidates, []);
  assert.equal(inbox.dismissed, true);
  assert.equal(receiveSniffCandidate(inbox, linked), inbox);

  const next = toSniffCandidate(
    candidate({ pageUrl: "http://127.0.0.1:53001/next.html" }),
  );
  assert.ok(next);
  inbox = receiveSniffCandidate(inbox, next);
  assert.equal(inbox.dismissed, false);
  assert.deepEqual(inbox.candidates, [next]);
  assert.equal(receiveSniffCandidate(inbox, candidate({ session: 6 })), inbox);
});

test("labels hide signed queries without changing the selectable URL", () => {
  const media =
    "https://cdn.example/films/trailer.mp4?signature=Q%2B9&expires=42";
  const parsed = toSniffCandidate(candidate({ url: media, source: "link" }));
  assert.ok(parsed);
  assert.equal(sniffCandidateLabel(parsed), "cdn.example/films/trailer.mp4");
  assert.equal(parsed.url, media);

  const current = receiveSniffCandidate(createSniffInbox(), parsed);
  const reopened = toSniffCandidate(
    candidate({ session: 8, url: TRAILER, source: "link" }),
  );
  assert.ok(reopened);
  const fresh = receiveSniffCandidate(current, reopened);
  assert.deepEqual(fresh.candidates, [reopened]);
  assert.equal(receiveSniffCandidate(fresh, parsed), fresh);
});

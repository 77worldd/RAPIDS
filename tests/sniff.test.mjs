import assert from "node:assert/strict";
import { test } from "node:test";
import {
  createSniffInbox,
  dismissSniffInbox,
  receiveSniffCandidate,
  selectSniffCandidate,
  sniffCandidateLabel,
  sniffPageMetadata,
  toInspectorUrl,
  toSniffCandidate,
} from "../src/lib/sniff.ts";

const PAGE = "http://127.0.0.1:53001/page.html";
const CLIP = "http://127.0.0.1:53001/clip.mp4?signature=Q%2B9";
const TRAILER = "http://127.0.0.1:53001/trailer.mp4";

const EMPTY_META = {
  title: null,
  poster: null,
  height: null,
  duration: null,
  pageTitle: null,
  siteName: null,
  ogTitle: null,
  twitterTitle: null,
  ogImage: null,
  twitterImage: null,
};

function candidate(changes = {}) {
  return {
    session: 7,
    pageUrl: PAGE,
    url: CLIP,
    source: "video",
    meta: { ...EMPTY_META },
    ...changes,
  };
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

test("inspector metadata is bounded and duplicate URLs gain missing values", () => {
  const parsed = toSniffCandidate(
    candidate({
      meta: {
        ...EMPTY_META,
        title: "  The   Movie  ",
        poster: "http://127.0.0.1:53001/poster.png#fragment",
        height: 0,
        duration: Number.NaN,
        pageTitle: "The Movie (2024) - Example Site",
        siteName: "Example Site",
        ogTitle: "The Movie (2024)",
        ogImage: "javascript:alert(1)",
        twitterImage: "http://127.0.0.1:53001/twitter.png",
      },
    }),
  );
  assert.ok(parsed);
  assert.equal(parsed.meta.title, "The Movie");
  assert.equal(parsed.meta.poster, "http://127.0.0.1:53001/poster.png");
  assert.equal(parsed.meta.height, null);
  assert.equal(parsed.meta.duration, null);
  assert.equal(parsed.meta.ogTitle, "The Movie (2024)");
  assert.equal(parsed.meta.ogImage, null);
  assert.equal(parsed.meta.twitterImage, "http://127.0.0.1:53001/twitter.png");

  const richer = toSniffCandidate(
    candidate({
      meta: {
        ...EMPTY_META,
        title: "The Movie",
        height: 1080,
        duration: 91.5,
        pageTitle: "The Movie (2024) - Example Site",
      },
    }),
  );
  assert.ok(richer);
  let inbox = receiveSniffCandidate(createSniffInbox(), parsed);
  inbox = receiveSniffCandidate(inbox, richer);
  assert.equal(inbox.candidates.length, 1);
  assert.equal(inbox.candidates[0].meta.title, "The Movie");
  assert.equal(inbox.candidates[0].meta.height, 1080);
  assert.equal(inbox.candidates[0].meta.duration, 91.5);
  assert.equal(inbox.candidates[0].meta.poster, parsed.meta.poster);

  assert.equal(
    toSniffCandidate({ ...candidate(), meta: "not-a-record" }),
    null,
  );
});

test("page metadata prefers the first non-null value across candidates", () => {
  const first = toSniffCandidate(
    candidate({
      meta: {
        ...EMPTY_META,
        pageTitle: "The Movie (2024) - Example Site",
        ogTitle: null,
        ogImage: "http://127.0.0.1:53001/poster-wide.png",
      },
    }),
  );
  const second = toSniffCandidate(
    candidate({
      url: TRAILER,
      meta: {
        ...EMPTY_META,
        pageTitle: "A different title",
        ogTitle: "The Movie (2024)",
        twitterTitle: "The Movie (2024)",
        twitterImage: "http://127.0.0.1:53001/twitter.png",
      },
    }),
  );
  assert.ok(first);
  assert.ok(second);
  assert.deepEqual(sniffPageMetadata([first, second]), {
    pageTitle: "The Movie (2024) - Example Site",
    siteName: null,
    ogTitle: "The Movie (2024)",
    twitterTitle: "The Movie (2024)",
    ogImage: "http://127.0.0.1:53001/poster-wide.png",
    twitterImage: "http://127.0.0.1:53001/twitter.png",
  });
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

test("receiving media is passive and unlisted choices cannot prefill a URL", () => {
  const first = candidate();
  const inbox = receiveSniffCandidate(createSniffInbox(), first);
  assert.deepEqual(inbox.candidates, [first]);
  assert.equal(inbox.dismissed, false);

  const unlisted = candidate({ url: TRAILER, source: "link" });
  const choice = selectSniffCandidate(inbox, unlisted);
  assert.equal(choice.url, null);
  assert.equal(choice.inbox, inbox);
});

test("explicit selection retains the exact signed URL and mutes that page", () => {
  const signed = candidate({
    url: "https://cdn.example/trailer.mp4?signature=Q%2B9&expires=42",
    source: "link",
  });
  const inbox = receiveSniffCandidate(createSniffInbox(), signed);
  const choice = selectSniffCandidate(inbox, signed);
  assert.equal(choice.url, signed.url);
  assert.equal(choice.inbox.dismissed, true);
  assert.deepEqual(choice.inbox.candidates, []);
  assert.equal(receiveSniffCandidate(choice.inbox, signed), choice.inbox);
  assert.equal(selectSniffCandidate(choice.inbox, signed).url, null);
});

test("a stale page or session cannot select a matching URL", () => {
  const old = candidate();
  const current = candidate({ session: 8, pageUrl: TRAILER });
  const inbox = receiveSniffCandidate(
    receiveSniffCandidate(createSniffInbox(), old),
    current,
  );
  for (const stale of [
    old,
    candidate({ session: 8 }),
    candidate({
      session: 8,
      pageUrl: TRAILER,
      source: "link",
    }),
  ]) {
    const choice = selectSniffCandidate(inbox, stale);
    assert.equal(choice.url, null);
    assert.equal(choice.inbox, inbox);
  }
});

test("a new navigation may be explicitly selected after dismissal", () => {
  const first = candidate();
  const dismissed = dismissSniffInbox(
    receiveSniffCandidate(createSniffInbox(), first),
  );
  const next = candidate({
    pageUrl: "http://127.0.0.1:53001/next.html",
    url: TRAILER,
    source: "link",
  });
  const inbox = receiveSniffCandidate(dismissed, next);
  assert.equal(inbox.dismissed, false);
  assert.equal(selectSniffCandidate(inbox, first).url, null);
  const choice = selectSniffCandidate(inbox, next);
  assert.equal(choice.url, TRAILER);
  assert.equal(choice.inbox.dismissed, true);
});

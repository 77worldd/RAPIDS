import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import vm from "node:vm";
import {
  buildMediaItems as buildTypeScript,
  normalizeMediaStem as normalizeStemTypeScript,
  resolveMediaTitle as resolveTitleTypeScript,
} from "../src/lib/media.ts";

const extensionSource = readFileSync(
  new URL("../extension/media.js", import.meta.url),
  "utf8",
);
const context = vm.createContext({ URL });
vm.runInContext(extensionSource, context, { filename: "media.js" });
const extension = context.RapidsMedia;

const PAGE_URL = "https://example.com/watch/the-movie";

function candidate(name, extra = {}) {
  return {
    url: `https://cdn.example.com/${encodeURIComponent(name)}`,
    name,
    ...extra,
  };
}

function at(url, name, extra = {}) {
  return { url, name, ...extra };
}

function build(candidates, overrides = {}) {
  return buildTypeScript({
    pageUrl: PAGE_URL,
    pageTitle: "The Movie (2024) - Site",
    siteName: "Site",
    candidates,
    ...overrides,
  });
}

function labels(item) {
  return item.variants.map((variant) => variant.label);
}

test("three quality responses become one item with three variants", () => {
  const items = build([
    candidate("CMAF_VP9_480.mp4"),
    candidate("CMAF_VP9_720.mp4"),
    candidate("CMAF_VP9_1080.mp4"),
  ]);
  assert.equal(items.length, 1);
  assert.equal(items[0].variants.length, 3);
  assert.deepEqual(labels(items[0]), ["1080p", "720p", "480p"]);
  assert.equal(items[0].kind, "video");
  assert.equal(
    items[0].selectedVariantUrl,
    "https://cdn.example.com/CMAF_VP9_1080.mp4",
  );
});

test("unrelated files on one page stay separate", () => {
  const items = build([candidate("movie.mp4"), candidate("trailer.mp4")]);
  assert.equal(items.length, 2);
  assert.deepEqual(items.map((item) => item.technicalName).sort(), [
    "movie.mp4",
    "trailer.mp4",
  ]);
});

test("episode tokens are not stripped", () => {
  const items = build([
    candidate("show.s01e01.mkv"),
    candidate("show.s01e02.mkv"),
  ]);
  assert.equal(items.length, 2);
});

test("fragment-only duplicates appear once", () => {
  const items = build([
    at("https://cdn.example.com/movie_1080.mp4#start", "movie_1080.mp4"),
    at("https://cdn.example.com/movie_1080.mp4#end", "movie_1080.mp4"),
  ]);
  assert.equal(items.length, 1);
  assert.equal(items[0].variants.length, 1);
});

test("query strings may differ while the stem still groups", () => {
  const items = build([
    at("https://cdn.example.com/movie_1080.mp4?token=one", "movie_1080.mp4"),
    at("https://cdn.example.com/movie_720.mp4?token=two", "movie_720.mp4"),
  ]);
  assert.equal(items.length, 1);
  assert.equal(items[0].variants.length, 2);
  assert.equal(
    items[0].variants[0].url,
    "https://cdn.example.com/movie_1080.mp4?token=one",
  );
});

test("audio-only sources join the matching video item", () => {
  const items = build([
    candidate("movie_1080.mp4"),
    candidate("movie_720.mp4"),
    candidate("movie_audio_128.m4a"),
  ]);
  assert.equal(items.length, 1);
  assert.equal(items[0].variants.length, 3);
  const audio = items[0].variants.find((variant) => variant.kind === "audio");
  assert.equal(audio.label, "Audio 128 kbps");
  assert.equal(audio.format, "M4A");
  assert.equal(audio.hasAudio, true);
});

test("a document with the same stem is a separate item", () => {
  const items = build([candidate("movie.mp4"), candidate("movie.srt")]);
  assert.equal(items.length, 2);
  assert.equal(items.filter((item) => item.kind === "file").length, 1);
});

test("variants are capped at eight after sorting", () => {
  const heights = [144, 240, 360, 480, 540, 576, 720, 1080, 1440, 2160];
  const candidates = heights.map((height, index) =>
    at(`https://cdn.example.com/clip.mp4?v=${index}`, "clip.mp4", { height }),
  );
  const items = build(candidates);
  assert.equal(items.length, 1);
  assert.equal(items[0].variants.length, extension.MAX_VARIANTS);
  assert.deepEqual(
    items[0].variants.map((variant) => variant.height),
    [2160, 1440, 1080, 720, 576, 540, 480, 360],
  );
});

test("invalid pages, credentials, and manifests are ignored", () => {
  assert.deepEqual(
    build([candidate("movie.mp4")], { pageUrl: "file:///x" }),
    [],
  );
  assert.deepEqual(
    build([at("https://user:secret@cdn.example.com/movie.mp4", "movie.mp4")]),
    [],
  );
  assert.deepEqual(
    build([candidate("playlist.m3u8"), candidate("manifest.mpd")]),
    [],
  );
});

test("a specific media title wins over page metadata", () => {
  const items = build(
    [candidate("movie_1080.mp4", { title: "Official Clip" })],
    { ogTitle: "Open Graph Movie" },
  );
  assert.equal(items[0].title, "Official Clip");
});

test("og:title is preferred over the page title", () => {
  const items = build([candidate("movie_1080.mp4")], {
    ogTitle: "Open Graph Movie",
  });
  assert.equal(items[0].title, "Open Graph Movie");
});

test("twitter:title is used when og:title is absent", () => {
  const items = build([candidate("movie_1080.mp4")], {
    twitterTitle: "Twitter Movie",
  });
  assert.equal(items[0].title, "Twitter Movie");
});

test("the page title keeps the movie name and drops the site suffix", () => {
  const items = build([candidate("movie_1080.mp4")]);
  assert.equal(items[0].title, "The Movie (2024)");
});

test("the filename is humanized when no page title exists", () => {
  const items = build([candidate("CMAF_VP9_480.mp4")], {
    pageTitle: null,
    siteName: null,
  });
  assert.equal(items[0].title, "CMAF VP9 480");
});

test("the cleaned URL stem is the last title fallback", () => {
  assert.equal(
    resolveTitleTypeScript({
      fileName: null,
      url: "https://cdn.example.com/My_Movie_1080p.mp4",
    }),
    "My movie",
  );
});

test("generic media titles never become the item title", () => {
  const items = build([candidate("movie_1080.mp4", { title: "Video" })], {
    pageTitle: "Video",
  });
  assert.equal(items[0].title, "movie 1080");
});

test("quality labels never become the item title", () => {
  const items = build(
    [
      candidate("movie_1080.mp4", { title: "1080p" }),
      candidate("movie_720.mp4", { title: "720p" }),
    ],
    { ogTitle: "The Movie" },
  );
  assert.equal(items[0].title, "The Movie");
});

test("poster, Open Graph image, then placeholder", () => {
  const poster = build([candidate("movie_1080.mp4")], {
    poster: "https://cdn.example.com/poster.jpg",
    ogImage: "https://cdn.example.com/og.jpg",
  });
  assert.equal(poster[0].thumbnailKind, "poster");
  assert.equal(poster[0].thumbnailUrl, "https://cdn.example.com/poster.jpg");

  const og = build([candidate("movie_1080.mp4")], {
    ogImage: "https://cdn.example.com/og.jpg",
  });
  assert.equal(og[0].thumbnailKind, "og");

  const none = build([candidate("movie_1080.mp4")]);
  assert.equal(none[0].thumbnailKind, "none");
  assert.equal(none[0].thumbnailUrl, null);
});

test("the technical name follows the default variant", () => {
  const items = build([
    candidate("movie_480.mp4"),
    candidate("movie_1080.mp4"),
  ]);
  assert.equal(items[0].technicalName, "movie_1080.mp4");
});

test("item ids are stable and unique per group", () => {
  const first = build([candidate("movie.mp4"), candidate("trailer.mp4")]);
  const second = build([candidate("movie.mp4"), candidate("trailer.mp4")]);
  assert.notEqual(first[0].id, first[1].id);
  assert.deepEqual(
    first.map((item) => item.id),
    second.map((item) => item.id),
  );
});

test("the default audio variant has the highest bitrate", () => {
  const items = build([
    candidate("movie_audio_64.m4a"),
    candidate("movie_audio_320.m4a"),
    candidate("movie_audio_128.m4a"),
  ]);
  assert.equal(items.length, 1);
  assert.equal(items[0].kind, "audio");
  assert.deepEqual(labels(items[0]), [
    "Audio 320 kbps",
    "Audio 128 kbps",
    "Audio 64 kbps",
  ]);
  assert.equal(
    items[0].selectedVariantUrl,
    "https://cdn.example.com/movie_audio_320.m4a",
  );
});

test("a lone 4K variant is selected", () => {
  const items = build([candidate("movie_2160p.mp4")]);
  assert.equal(items[0].variants[0].height, 2160);
  assert.equal(items[0].selectedVariantUrl, items[0].variants[0].url);
});

test("4K wins over 8K when nothing is at or below 1080p", () => {
  const items = build([
    candidate("movie_4320p.mp4"),
    candidate("movie_2160p.mp4"),
  ]);
  assert.deepEqual(labels(items[0]), ["4320p", "2160p"]);
  assert.equal(
    items[0].selectedVariantUrl,
    "https://cdn.example.com/movie_2160p.mp4",
  );
});

test("an unknown quality is labelled Original", () => {
  const items = build([candidate("movie.mp4")]);
  assert.equal(items[0].variants[0].label, "Original");
  assert.equal(items[0].variants[0].height, null);
  assert.equal(items[0].selectedVariantUrl, items[0].variants[0].url);
});

test("video-only sources are marked without inventing other audio data", () => {
  const items = build([candidate("movie_1080p_video-only.mp4")]);
  assert.equal(items[0].variants[0].hasAudio, false);
  assert.equal(items[0].variants[0].label, "1080p");
});

test("metadata is reported only when it is known", () => {
  const items = build([candidate("movie.webm")]);
  const variant = items[0].variants[0];
  assert.equal(variant.size, null);
  assert.equal(variant.codec, null);
  assert.equal(variant.format, "WebM");
  assert.equal(variant.hasAudio, null);
  assert.equal(variant.duration, null);
});

test("a video poster belongs to its own item, not the whole page", () => {
  const items = build(
    [
      candidate("movie_1080.mp4", {
        poster: "https://cdn.example.com/movie-poster.jpg",
      }),
      candidate("trailer_720.mp4"),
    ],
    { poster: "https://cdn.example.com/page-poster.jpg" },
  );
  const movie = items.find((item) => item.technicalName === "movie_1080.mp4");
  const trailer = items.find(
    (item) => item.technicalName === "trailer_720.mp4",
  );
  assert.equal(movie.thumbnailUrl, "https://cdn.example.com/movie-poster.jpg");
  assert.equal(movie.thumbnailKind, "poster");
  assert.equal(trailer.thumbnailUrl, "https://cdn.example.com/page-poster.jpg");
  assert.equal(trailer.thumbnailKind, "poster");
});

test("duration is kept when the page reports it", () => {
  const items = build([
    candidate("movie_1080.mp4", { duration: 3725.4, height: 1080 }),
  ]);
  assert.equal(items[0].variants[0].duration, 3725.4);
});

test("codec, size, format, and source survive when supplied", () => {
  const items = build([
    candidate("Clip.VP9.1080p.webm", {
      size: 123456,
      source: "videoSource",
      contentType: "video/webm",
    }),
  ]);
  const variant = items[0].variants[0];
  assert.equal(variant.codec, "VP9");
  assert.equal(variant.size, 123456);
  assert.equal(variant.format, "WebM");
  assert.equal(variant.source, "videoSource");
  assert.equal(variant.height, 1080);
});

test("a duplicate URL gains quality metadata from a later candidate", () => {
  const url = "https://cdn.example.com/movie.mp4";
  const items = build([
    at(url, "movie.mp4", { kind: "video", source: "link" }),
    at(url, "movie.mp4", {
      kind: "video",
      height: 1080,
      source: "videoSource",
    }),
  ]);
  assert.equal(items.length, 1);
  assert.equal(items[0].variants.length, 1);
  assert.equal(items[0].variants[0].height, 1080);
  assert.equal(items[0].variants[0].label, "1080p");
  assert.equal(items[0].variants[0].source, "videoSource");
});

test("the extension mirror returns the same model as TypeScript", () => {
  const fixtures = [
    {
      pageUrl: PAGE_URL,
      pageTitle: "The Movie (2024) - Site",
      siteName: "Site",
      ogTitle: "Open Graph Movie",
      poster: "https://cdn.example.com/poster.jpg",
      candidates: [
        candidate("movie_480.mp4", { size: 1000 }),
        candidate("movie_1080.mp4", { size: 2000, duration: 3600 }),
        candidate("movie_audio_128.m4a", { size: 300 }),
        candidate("trailer_720.mp4", {
          poster: "https://cdn.example.com/trailer.jpg",
        }),
      ],
    },
    {
      pageUrl: "https://example.org/episode",
      twitterImage: "https://cdn.example.org/cover.jpg",
      candidates: [
        at("https://cdn.example.org/show.s01e01.mkv", "show.s01e01.mkv"),
        at("https://cdn.example.org/show.s01e02.mkv", "show.s01e02.mkv"),
      ],
    },
    {
      pageUrl: PAGE_URL,
      pageTitle: "Video",
      candidates: [
        at("https://cdn.example.com/no-extension", null, {
          kind: "file",
          source: "navigation",
        }),
      ],
    },
    {
      pageUrl: PAGE_URL,
      ogTitle: "Parity Movie",
      candidates: [candidate("movie_480.mp4", { title: "480p" })],
    },
  ];
  for (const fixture of fixtures) {
    assert.deepEqual(
      JSON.parse(JSON.stringify(buildTypeScript(fixture))),
      JSON.parse(JSON.stringify(extension.buildMediaItems(fixture))),
    );
  }
});

test("the stem parser mirrors the extension in both languages", () => {
  const names = [
    "CMAF_VP9_480.mp4",
    "show.s01e01.mkv",
    "movie_audio_128.m4a",
    "movie_1080p_video-only.mp4",
    "The.Movie.2024.2160p.HDR.x265.mkv",
    "clip.mp4",
  ];
  for (const name of names) {
    assert.equal(
      normalizeStemTypeScript(name),
      extension.normalizeMediaStem(name),
    );
    assert.equal(
      extension.normalizeMediaStem(name),
      normalizeStemTypeScript(name),
    );
  }
  assert.equal(normalizeStemTypeScript("show.s01e01.mkv"), "show s01e01");
  assert.equal(normalizeStemTypeScript("CMAF_VP9_480.mp4"), "cmaf");
  assert.equal(normalizeStemTypeScript("movie_audio_128.m4a"), "movie");
});

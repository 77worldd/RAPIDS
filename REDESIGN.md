# RAPIDS redesign — specification and acceptance criteria

Status: proposal, pending three decisions in [Open decisions](#open-decisions).

## 1. Problem

Two surfaces expose raw streams as unrelated rows:

- Extension popup lists every sniffed URL (`CMAF_VP9_480.mp4`, `CMAF_VP9_1080.mp4`, `audio_128.m4a`) as a separate card with no title, thumbnail, or grouping.
- Desktop inspector (`MediaCandidatePopup`) lists the same way and only has `pageUrl`, `url`, `source` — no size, type, or name.

The visual identity is also inconsistent: emerald accents are hard-coded in ~20 files across the app and extension instead of flowing from one token set.

## 2. What data actually exists today

| Field                         | Extension                      | Desktop inspector | Notes                                   |
| ----------------------------- | ------------------------------ | ----------------- | --------------------------------------- |
| URL                           | yes                            | yes               | validated HTTP(S), fragments stripped   |
| Page URL                      | yes                            | yes               |                                         |
| Source kind                   | yes                            | yes               | video / videoSource / link / navigation |
| Filename                      | yes (Content-Disposition, URL) | no                | desktop derives from URL at add time    |
| Size                          | yes (`Content-Length`)         | no                |                                         |
| MIME type                     | yes (`Content-Type`)           | no                |                                         |
| Page title                    | no                             | no                | not captured                            |
| `og:title` / `og:image`       | no                             | no                | not captured                            |
| Poster                        | no                             | no                | not captured                            |
| Resolution                    | no                             | no                | only inferable from filename tokens     |
| Duration / fps / audio tracks | no                             | no                | requires media probing                  |

Implication: previews and metadata must be sourced from the page itself (DOM), not from the download URL. Anything that requires reading the media bytes is a separate, larger project.

## 3. Design decisions

### 3.1 One media item, many variants

Introduce a shared media model used by both surfaces:

```ts
type MediaKind = "video" | "audio" | "file";

interface MediaVariant {
  url: string;
  height: number | null; // 2160, 1440, 1080, 720, 480...
  label: string; // "1080p", "720p", "Audio 128 kbps", "Original"
  format: string; // "MP4", "WebM", "MKV", "M4A", "Unknown"
  size: number | null;
  hasAudio: true | false | null;
  codec: string | null; // parsed from filename when present
  source: SniffSource;
}

interface MediaItem {
  id: string; // stable hash of pageUrl + normalized title
  title: string; // human title
  technicalName: string; // filename of the default variant
  pageUrl: string;
  host: string;
  kind: MediaKind;
  thumbnailUrl: string | null;
  thumbnailKind: "poster" | "og" | "none";
  variants: MediaVariant[];
  selectedVariantUrl: string;
}
```

Grouping rules (must not merge unrelated files):

1. Same `pageUrl` and same `kind`.
2. Normalized stem equality after removing quality/codec/audio tokens
   (`1080p`, `720p`, `4k`, `2160`, `h264`, `vp9`, `av1`, `aac`, `dts`, `audio`, `video-only`, bitrate suffixes).
3. Different query strings are allowed to group only when the stem matches.
4. Anything that fails the stem test stays its own item.
5. Maximum 8 variants per item; extras are dropped, not merged.

### 3.2 Title resolution order

1. `<video title>` / `aria-label` / `<track label>` when specific.
2. `og:title` or `twitter:title`.
3. `document.title` with site suffix removed (`"Movie (2024) - Site"` → `"Movie (2024)"`).
4. `Content-Disposition` filename without extension.
5. Cleaned URL stem (`CMAF_VP9_480` → `CMAF VP9 480`, only after tokens are stripped).

The technical filename is always retained and shown as secondary text.

### 3.3 Thumbnails

- `video[poster]` when present and HTTP(S).
- `og:image` / `twitter:image` when it resolves to HTTP(S).
- Otherwise a token-driven monochrome placeholder (no stock art).
- No canvas frame capture in this phase: cross-origin video taints the canvas and silently returns blank images. A later phase can add it behind a CORS check.

### 3.4 Monochrome system

Single token set, duplicated into `src/index.css` and `extension/theme.css` (no build step in the extension). Values switch with `prefers-color-scheme` and can be overridden by an explicit `data-theme` attribute.

| Token           | Dark               | Light              | Use                        |
| --------------- | ------------------ | ------------------ | -------------------------- |
| `--canvas`      | `#0a0a0b`          | `#fafafa`          | window background          |
| `--panel`       | `#111113`          | `#ffffff`          | cards, sidebar, popup body |
| `--elevated`    | `#18181b`          | `#f4f4f5`          | menus, selected rows       |
| `--line`        | `#2a2a2d`          | `#e4e4e7`          | dividers, borders          |
| `--line-strong` | `#3a3a3f`          | `#d4d4d8`          | hover borders              |
| `--ink`         | `#f4f4f5`          | `#0a0a0b`          | primary text               |
| `--ink-soft`    | `#c8c8cc`          | `#3f3f46`          | secondary text             |
| `--muted`       | `#8b8b90`          | `#71717a`          | metadata                   |
| `--faint`       | `#5c5c62`          | `#a1a1aa`          | disabled, hints            |
| `--accent`      | `#ffffff`          | `#0a0a0b`          | primary button fill        |
| `--accent-ink`  | `#0a0a0b`          | `#fafafa`          | primary button text        |
| `--overlay`     | `rgb(0 0 0 / .55)` | `rgb(0 0 0 / .35)` | dialog scrim               |

The "accent" is inverted ink, not a color: a white button on dark, a black button on light. Hover is `opacity: .88`, never a hue shift.

Component mapping (replaces raw zinc classes):

| Current                              | Semantic                    |
| ------------------------------------ | --------------------------- |
| `bg-zinc-950`, `bg-canvas`           | `bg-canvas`                 |
| `bg-zinc-900`, `bg-panel`            | `bg-panel`                  |
| `bg-zinc-800`, `bg-elevated`         | `bg-elevated`               |
| `border-zinc-700/800`, `border-line` | `border-line`               |
| `text-zinc-100/200`                  | `text-ink`                  |
| `text-zinc-300/400`                  | `text-ink-soft`             |
| `text-zinc-500`                      | `text-muted`                |
| `text-zinc-600`                      | `text-faint`                |
| `bg-emerald-500 text-zinc-950`       | `bg-accent text-accent-ink` |
| `ring-emerald-*`                     | `ring-accent`               |
| `accent-emerald-500`                 | `accent-[--accent]`         |

Theme selection order: `data-theme` on `<html>` (user override) → `prefers-color-scheme` → dark. Setting lives in **Settings → Appearance** with `System / Light / Dark`, default `System`. The extension follows the same preference through `chrome.storage.local` (`theme` key), default `system`.

Rules:

- No hue anywhere. `emerald`, `teal`, `green`, `amber`, `lime` are removed from class names, CSS variables, inline styles, and the extension badge.
- Status is conveyed by icon + label + fill, never by hue:
  - Completed: filled check, white text, solid bar at 100%.
  - Failed: X icon, outlined pill, bar replaced by a static error strip.
  - Paused: two-bar icon, hatched progress track.
  - Queued: dashed circle, dim bar.
  - Downloading: animated white bar on gray track.
- Focus rings become `outline: 2px solid #f4f4f5` at 2px offset (visible on every surface).
- Selected state uses a 1px white border + `#18181b` fill, not a color accent.
- Success/warning/error toasts remain distinguishable by icon and wording; all three are grayscale.

### 3.5 Layout and interaction

- Spacing scale: 4 / 8 / 12 / 16 / 24 / 32, no arbitrary values.
- Type scale: 11 / 12 / 13 / 15 / 18 / 24, weights 400 / 500 / 600 only.
- Radius scale: 6 (controls), 10 (cards), 14 (dialogs). No pills larger than 8 except tags.
- Extension popup: 380px wide, 3-line item (thumb + title + meta), quality menu expands inline, max height 520px, keyboard navigable.
- Desktop: media picker becomes a resizable panel with a grid/list toggle; preview appears on selection (desktop) and on hover (extension).
- Motion: 120–180ms ease-out for hover/press, 200ms for panels; honor `prefers-reduced-motion`.

## 4. Deliverables

| #   | Deliverable                                                            | Files                                                                                                       |
| --- | ---------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------- |
| D1  | Token layer + monochrome migration                                     | `src/index.css`, all `src/**/*.tsx`, `extension/theme.css`, `extension/popup.css`, `extension/options.css`  |
| D2  | Shared grouping/quality parser (TS + JS mirror)                        | `src/lib/media.ts`, `extension/media.js`, tests for both                                                    |
| D3  | DOM metadata capture (title, poster, og:*, dimensions, duration)       | `extension/content.js`, `extension/background.js`                                                           |
| D4  | Desktop media picker with previews and quality menu                    | `src/components/tasks/MediaPicker.tsx`, `MediaCandidatePopup.tsx`, `src/views/TasksView.tsx`, `src/state/*` |
| D5  | Extension popup redesign (grouped cards, inline quality menu, preview) | `extension/popup.html`, `popup.css`, `popup.js`                                                             |
| D6  | Handoff carries selected variant + title                               | `extension/handoff.js`, `src/lib/deeplink.ts`, `src-tauri/src/deeplink.rs` (only if `n` semantics change)   |
| D7  | Docs + screenshots                                                     | `README.md`, `REDESIGN.md`                                                                                  |

Out of scope for this pass: HLS/DASH manifest parsing, ffprobe/container inspection, frame extraction, player integration.

## 5. Acceptance criteria

### Grouping and titles

- **AC-01** Given three responses `CMAF_VP9_480.mp4`, `CMAF_VP9_720.mp4`, `CMAF_VP9_1080.mp4` on one page, the UI shows **one** item with three quality options.
- **AC-02** Given `movie.mp4` and `trailer.mp4` on one page, the UI shows two items.
- **AC-03** A page with `show.s01e01.mkv` and `show.s01e02.mkv` shows two items (episode tokens are not stripped).
- **AC-04** Title uses `og:title` when present; otherwise the cleaned page title; otherwise the file stem.
- **AC-05** The raw filename is always reachable as secondary text or tooltip.
- **AC-06** Duplicate URLs (fragment-only difference) appear once.

### Quality menu

- **AC-07** One dropdown per item lists variants sorted by height descending.
- **AC-08** Each row shows label, format, size (when known), and an "audio included" vs "video only" marker.
- **AC-09** Default selection is the highest variant at or below 1080p; if only 4K exists, 4K is selected.
- **AC-10** Changing the selection updates the primary action and the handed-off URL.
- **AC-11** Audio-only sources appear as variants inside the item, not as separate cards.
- **AC-12** A video-only variant shows a "no audio" warning before download.
- **AC-13** The menu is operable by keyboard (`Up`/`Down`/`Enter`/`Esc`) and focus returns to the trigger.

### Previews and metadata

- **AC-14** An item renders `video[poster]` as its thumbnail when available.
- **AC-15** With no poster, `og:image` is used; with neither, a monochrome placeholder renders.
- **AC-16** A broken thumbnail URL falls back to the placeholder without a broken-image icon.
- **AC-17** Resolution/format/size/duration are shown only when known; no fabricated values.
- **AC-18** Hover (extension) or selection (desktop) reveals title, host, variant count, and metadata.

### Monochrome

- **AC-19** A source scan finds zero occurrences of `emerald|green|teal|lime|#10b981|#059669|#34d399|rgb(16 185 129)` in `src/` and `extension/`.
- **AC-20** The toolbar badge uses a neutral background (`#18181b`) and remains legible in both browser themes.
- **AC-21** Every status (queued/downloading/paused/completed/failed) is distinguishable in a grayscale screenshot.
- **AC-22** Focus-visible rings are present on all interactive elements and pass 3:1 contrast against the adjacent surface.
- **AC-23** Body text contrast is ≥ 4.5:1; secondary text ≥ 3:1.
- **AC-24** No hue survives in hover, active, selected, disabled, progress, toggle, checkbox, menu, dialog, toast, or empty states.

### Consistency and behavior

- **AC-25** Extension and desktop use identical variant labels, ordering, and default-selection rules.
- **AC-26** Existing downloads, resume, queue limits, and deep-link behavior are unchanged (regression suite passes).
- **AC-27** The extension popup is fully usable at 360px width and 520px height with no horizontal scroll.
- **AC-28** The desktop media picker is usable at 720px and 1400px window widths.
- **AC-29** `prefers-reduced-motion` disables non-essential transitions.
- **AC-30** All existing tests pass; new tests cover grouping, title resolution, quality ordering, and monochrome token usage.

## 6. Test plan

- Unit: grouping table tests (12 cases), title resolution (8), variant ordering and default (6), both TS and extension JS.
- Visual: screenshots of tasks (5 statuses), add sheet, media picker, popup (empty/populated/menu open), options, settings, all in grayscale and color.
- Manual: load unpacked extension in Helium; sniff a page with three qualities; confirm one card, dropdown, handoff; verify the app opens the chosen URL.
- Regression: `cargo test --workspace`, `npm run lint`, `typecheck`, `test:deeplink`, `test:extension`, `test:add-draft`, `test:sniff`, `npm run build`.

## 7. Resolved decisions

1. **Theme scope** — ship both light and dark, default to `system`, with an explicit override in Settings and in extension Options.
2. **Thumbnail source** — page images only (`video[poster]`, `og:image`, `twitter:image`). No canvas capture, no byte probing.
3. **Streaming** — HLS/DASH stay out of scope. `.m3u8`/`.mpd` remain rejected as they are today.

Consequence: fps, audio-track count, and true codec cannot be shown. Resolution is shown only when it comes from the page (`videoWidth`) or a filename token; otherwise the label is "Original". This is deliberate: the UI must never invent metadata.

## 8. Phases and exit criteria

Each phase is a separate reviewed commit series; the app stays runnable after every phase.

### Phase 1 — Design system

Deliverables: D1 (tokens, light/dark, theme preference), extension `theme.css`, badge color.

Exit criteria: AC-19 … AC-24, AC-29; both themes screenshot-checked; no behavioral diff; `npm run build` passes.

### Phase 2 — Media model

Deliverables: D2 (grouping, titles, variant parsing, default selection) with mirrored TS/JS units and tests.

Exit criteria: AC-01 … AC-06, AC-07 … AC-09, AC-11, AC-25 (data layer); 26 new unit tests green.

### Phase 3 — Extension UX

Deliverables: D3, D5, D6.

Exit criteria: AC-10, AC-12 … AC-18, AC-27; manual pass in Helium on a three-quality fixture; handoff carries the selected variant.

### Phase 4 — Desktop UX

Deliverables: D4, D6.

Exit criteria: AC-07 … AC-18 on desktop, AC-26, AC-28; media picker reachable from the inspector and the Add sheet; queue/resume regression suite green.

### Phase 5 — Polish and proof

Deliverables: D7, screenshots, docs.

Exit criteria: full AC sweep, AC-30, README updated, screenshots committed under `docs/redesign/`.

## 9. Definition of done (per deliverable)

A deliverable is done only when all of the following hold:

- The behavior is reachable without developer tools (no hidden flags).
- Empty, loading, error, and disabled states exist and are styled.
- Keyboard navigation and visible focus work.
- Both themes render without unreadable contrast.
- Unit tests cover the pure logic; manual steps are written in the phase notes.
- No new runtime dependency in the frontend, no build step added to the extension.
- Existing download, queue, resume, and deep-link behavior is unchanged.

# RAPIDS — Architecture

RAPIDS is a fast, private, minimal download manager for Windows 11, built for personal
use. This document is the authoritative map of the codebase and is updated at the end of
every session.

**Scope until opened otherwise:** HTTP/HTTPS segmented downloads, queue/scheduler,
organization, encrypted vault, system integration, browser extension, FTP.
**Closed phase:** BitTorrent, HLS/M3U8, and media-site parsing remain out of scope until
the owner explicitly opens that phase.

---

## 1. Stack

| Layer | Technology | Version in repo |
|---|---|---|
| App shell | Tauri 2 (Rust host, WebView2) | CLI 2.12.3 / crate 2.12.4 |
| Async runtime | tokio (`rt-multi-thread`, `macros`, `fs`, `sync`, `time`, `io-util`, `signal`) | 1.x |
| HTTP engine | reqwest (rustls + native roots, streaming, HTTP/1.1 connections) | 0.12.x |
| Hashing | sha2 (SHA-256 verification) | 0.10.x |
| Serialization | serde + serde_json (manifests) | 1.x |
| CLI parsing | clap (derive) | 4.x |
| Test server | hand-rolled `std::net` thread server (`rapids-test-support`) | in-repo |
| Frontend | React 18 + TypeScript | 18.3.1 / 6.0.3 |
| Styling | Tailwind CSS (Vite plugin) | 4.3.3 |
| Motion | Framer Motion | 14.4.0 |
| Icons | lucide-react | 1.60.0 |
| Fonts | Inter, self-hosted via `@fontsource/inter` (zero network requests) | 5.3.0 |
| Bundler | Vite | 8.3.4 |
| Lint/format | ESLint (flat, type-aware) + typescript-eslint + Prettier | 10.13.0 / 8.72.0 |
| Storage (M2 onward) | SQLite via sqlx + sidecar JSON manifests | not yet attached |

Dependency policy: every entry above was approved before install (rule 9). No other
runtime dependencies exist.

---

## 2. Repository layout

```
RAPIDS/
├── package.json                # scripts: dev, build, typecheck, lint, format, gate:content, tauri
├── eslint.config.js            # flat config; type-aware rules for TS; no-console = error in app code
├── tsconfig.json               # single strict project: src/ + vite.config.ts
├── vite.config.ts              # react + tailwind plugins; dev server pinned to port 1420
├── scripts/content-gate.mjs    # mechanical enforcement of the repository content rules
├── assets/icon-1024.png        # original RAPIDS mark; source for `npx tauri icon`
├── Cargo.toml                  # cargo workspace: crates/* + src-tauri (shared target/, one Cargo.lock)
├── crates/
│   ├── rapids-engine/          # download engine (library)
│   │   ├── src/
│   │   │   ├── lib.rs          # public API: spawn_download, handles, re-exports
│   │   │   ├── config.rs       # EngineConfig with spec defaults (8→32, backoff, timeouts)
│   │   │   ├── error.rs        # EngineError: every failure with a human message
│   │   │   ├── events.rs       # DownloadState + EngineEvent stream contract
│   │   │   ├── manifest.rs     # sidecar JSON: versions, atomic save, resume gate
│   │   │   ├── workspace.rs    # dest.rapids/ dir: part file, manifest, OS file lock
│   │   │   ├── segmenter.rs    # exact-cover byte ranges + connection scaling decision
│   │   │   ├── hash.rs         # streamed SHA-256
│   │   │   ├── util.rs         # human bytes/duration formatting
│   │   │   ├── http/           # probe (ranges/validators/filenames) + fetch (workers)
│   │   │   └── coordinator.rs  # probe → plan → sessions → finalize state machine
│   │   └── tests/              # integration tests over the local test server
│   ├── rapids-cli/             # `rapids get|cancel` binary (live progress, Ctrl+C)
│   │   └── tests/kill_resume.rs# 100 MB kill-at-50% → resume → SHA-256 acceptance
│   └── rapids-test-support/    # deterministic local HTTP server + payload generator
├── src/                        # frontend (React shell)
│   ├── main.tsx                # root mount, Inter weights, StrictMode
│   ├── App.tsx                 # StoreProvider → AppShell
│   ├── index.css               # Tailwind entry, theme tokens, base typography, scrollbars
│   ├── types.ts                # shared contracts: ViewId, TagId, Task, TaskStatus, AppInfo
│   ├── config/
│   │   ├── navigation.ts       # sidebar sections/items (labels + icons) and viewLabel()
│   │   └── categories.ts       # default categories: Apps, Movies, Music, Images, Other
│   ├── state/
│   │   ├── StoreProvider.tsx   # context + reducer, hydration seam, persistence of UI prefs
│   │   └── selectors.ts        # per-view filtering, visible-list filtering, counts
│   ├── lib/
│   │   ├── ipc.ts              # the single typed invoke() wrapper
│   │   ├── uiPrefs.ts          # versioned localStorage for sidebar state (non-secret only)
│   │   └── cn.ts               # className joiner (no dependency)
│   ├── components/
│   │   ├── layout/AppShell.tsx # sidebar | (topbar / content) grid, page-level overflow lock
│   │   ├── sidebar/            # Sidebar, SidebarSection, SidebarItem, CountBadge, toggle, footer
│   │   ├── topbar/             # TopBar, ViewTitle, SearchBox, RefreshButton
│   │   ├── ui/                 # Button, IconButton, EmptyState, Skeleton
│   │   └── motion/variants.ts  # the single motion constant: 180ms ease-out
│   └── views/TasksView.tsx     # loading skeleton / empty / no-match states
└── src-tauri/                  # Rust host
    ├── Cargo.toml              # rapids 0.1.0; tauri, serde, serde_json, tokio
    ├── tauri.conf.json         # identity, window, CSP, icons
    ├── capabilities/default.json # least privilege: core:default for the main window
    ├── icons/                  # full icon set generated from assets/icon-1024.png
    └── src/
        ├── main.rs             # Windows subsystem entry → rapids_lib::run()
        ├── lib.rs              # builder + command registration
        └── commands.rs         # app_version → { name, version } from the host bundle
```

---

## 3. Module responsibilities

### Frontend shell
- **`AppShell`** owns layout geometry only: fixed sidebar, slim top bar, scroll-locked
  content column. No data.
- **`StoreProvider`** is the single state container (React context + reducer — no state
  library). It owns the hydration state machine (`loading → ready`), the current view,
  the search query, the task list, and sidebar collapse. Every future engine event
  enters the app by dispatching here.
- **`selectors.ts`** is the only place that knows view semantics (Running =
  status `downloading`, Unfinished = anything but `completed`, tag views match
  `task.category`, etc.). Both sidebar counts and the content list derive from it, so
  badges and titles can never disagree.
- **`Sidebar`** renders NAV_SECTIONS with live counts and an animated active marker;
  **`TopBar`** renders view title + count, search, refresh. `SearchBox` filters through
  the store; Escape and the clear button empty the query.
- **`TasksView`** maps hydration state to the three user-visible states: skeleton while
  counting, one-sentence empty state per view, or a no-match state with a working
  "Clear search" action.
- **`lib/ipc.ts`** is the only module that touches the host bridge. All future commands
  get a typed wrapper here — components never call `invoke` directly.
- **`motion/variants.ts`** holds the app's one motion constant (180ms ease-out). CSS
  hover/focus transitions are `duration-150 ease-out`. Nothing animates longer.

### Rust host
- **`lib.rs`** builds the Tauri app and registers commands; engine modules (scheduler,
  rate-limiter, protocol clients, vault) attach here with their milestones.
- **`commands.rs`** serves `app_version` from `package_info()` — the product name and
  version have one source of truth: `tauri.conf.json`.

### Engine (`crates/rapids-engine`)
- **`coordinator.rs`** is the state machine: probe → settle/plan → transfer sessions →
  finalize. It owns cancellation, pausing, checkpoints, progress/speed events, and the
  connection-scaling decision; workers never make policy calls.
- **`http/probe.rs`** discovers size, range support (header claim *and* behavioural
  206 confirmation), ETag/Last-Modified validators, and the filename
  (`filename*=` → `filename=` → URL segment), retrying transient statuses only.
- **`http/fetch.rs`** runs per-segment workers over a shared queue: ranged requests
  with `If-Range`, chunk writes before atomic progress stores, idle-read timeout,
  exponential backoff with hash jitter, and immediate response to control flags.
- **`manifest.rs` + `workspace.rs`** implement kill-safety: `written` values are
  conservative flushed low-water marks, the manifest is written temp-file → rename,
  and an OS-level file lock (auto-released when a process dies) prevents two
  processes from sharing one workspace.
- **`segmenter.rs`** tiles `[0,total)` into ≤32 exact-cover ranges and holds the
  pure scaling rule (add a connection only while the last window's gain ≥ 20%).
- **Engine is UI-agnostic**: hosts consume `spawn_download(spec, config)` →
  `(DownloadHandle, event receiver)`; `DownloadHandle` offers `pause`, `resume`,
  `cancel`, and a state watch.

### CLI (`crates/rapids-cli`)
- **`main.rs`** maps events to a terminal experience and exit codes
  (0 completed, 2 paused by first Ctrl+C, 3 failure, 130 second Ctrl+C).
- **`render.rs`** owns the single live progress line (terminal only; plain lines
  otherwise). `cancel` removes a workspace that is not locked by a live process.

---

## 4. Data formats and contracts

| Format | Location | Shape / rules |
|---|---|---|
| Host config | `src-tauri/tauri.conf.json` | identity `com.rapids.desktop`, productName `RAPIDS`, window 1180×760 (min 960×620), strict `csp` for release + `devCsp` for dev (inline scripts allowed only in dev for React Refresh; HMR socket whitelisted) |
| Capabilities | `src-tauri/capabilities/default.json` | `core:default` for window `main` only; feature permissions are added per milestone |
| IPC | `invoke('app_version')` | `AppInfo { name: string, version: string }`, wrapped in `lib/ipc.ts` |
| Store state | `src/state/StoreProvider.tsx` | `{ status: 'loading' \| 'ready', tasks, viewId, query, sidebarCollapsed }` with a closed action union |
| Task render contract | `src/types.ts` | `Task { id, fileName, status, totalBytes, speedBps, etaSeconds, category, tags, favorite, addedAt }`; `TaskStatus = queued \| downloading \| paused \| completed \| error` |
| Views | `src/types.ts` + `navigation.ts` | `CoreViewId` (7 task/scheduled views) ∪ `TagId` (5 categories) |
| UI prefs | `localStorage['rapids.ui']` | `{ v: 1, sidebarCollapsed: boolean }`, validated on read; non-secret UI state only — secrets never touch localStorage (rule 5 applies from the vault milestone onward) |
| Icons | `assets/icon-1024.png` → `npx tauri icon` | regenerate the set whenever the mark changes |
| Download sidecar | `<dest>.rapids/` next to the destination | `manifest.json` + `data.part` + `lock` (OS file lock, released automatically when the process dies) |
| Manifest | `<dest>.rapids/manifest.json` | `{ v, url, final_url, dest, total_bytes?, accepts_ranges, etag?, last_modified?, content_type?, filename?, expected_sha256?, observed_sha256?, created_at, segments: [{ i, start, end, written }], completed }` — written temp-file → atomic rename; `written` is a conservative flushed low-water mark |
| Resume gate | `manifest.can_resume` + part-file check | range support recorded, stored size equals the fresh probe size, part-file length equals that size, and a stored validator (ETag preferred, else Last-Modified) matches the fresh probe — otherwise wipe and restart |
| Engine events | `spawn_download` receiver (cap 64) | `StateChanged · Progress (coalescible) · Retrying · Completed · Failed · Cancelled`; progress carries downloaded/total/speed/ETA/connections/segment counts |
| CLI surface | `rapids get <url> [-o] [--sha256] [--connections] [--overwrite]`, `rapids cancel <path>` | exit codes: 0 completed · 2 paused by first Ctrl+C · 3 failed · 130 second Ctrl+C |

---

## 5. Design system

- Dark-first: background `#09090b` (zinc-950), text zinc-200/100, borders zinc-800.
- Accent: emerald — `emerald-500` for actions/progress, `emerald-400` for active text.
- Type: Inter (bundled), 13.5px base, `font-variant-numeric: tabular-nums` globally so
  every count/speed/percent column aligns.
- State colors: green = healthy download, amber = waiting/queued, red = error with a
  readable message and Retry, gray = paused (applied from M3 when rows exist).
- Motion: 150–200ms ease-out only (`motion/variants.ts` + Tailwind `duration-150
  ease-out`). Skeleton loaders render whenever `status = 'loading'`.
- Empty states: one helpful sentence, never a blank pane; an action button appears only
  when a real action exists (today: "Clear search"; the Add action arrives with the
  Add dialog in M3).
- Controls rule: a control exists in the DOM only when it has working behavior. The
  Add/Delete/Resume/Pause toolbar, Settings gear, and "+ New tag" are therefore
  deferred to M3 where their handlers are real.

---

## 6. Quality gates

Run before every commit:

| Gate | Command | Where |
|---|---|---|
| Lint (zero warnings) | `npm run lint` | sandbox (enforced per commit) |
| Types (strict, zero errors) | `npm run typecheck` | sandbox (enforced per commit) |
| Production build | `npm run build` | sandbox (enforced per commit) |
| Content rules | `npm run gate:content` | sandbox (enforced per commit) |
| Rust format | `cargo fmt --all` once, then `cargo fmt --all -- --check` | Windows machine (see below) |
| Rust lints | `cargo clippy --workspace --all-targets -- -D warnings` | Windows machine (see below) |
| Rust tests | `cargo test --workspace` | Windows machine (see below) |

The sandbox has no Rust toolchain and no route to crates.io (verified by probing), and
no GUI libraries; the target platform is Windows 11 anyway. The three `cargo` gates
therefore run on the owner's machine — this split was explicitly approved. Any finding
they produce is fixed before the session closes.

`scripts/content-gate.mjs` scans authored text files and fails the build on task
markers, staged-language fillers, deferred-feature phrasing, and invented-data markers
(it exempts itself and the generated lockfile). References to
third-party products are policed by review: the repo contains only RAPIDS and its own
story.

Frontend console rule: zero errors — bridge calls are wrapped so the shell degrades
silently and correctly when opened outside the desktop host.

---

## 7. Milestone roadmap

| # | Milestone | Status |
|---|---|---|
| M0 | Scaffold: Tauri 2 + React + TS + Tailwind shell per design | **complete** (session 1) |
| M1 | Engine core: segmented HTTP, kill-safe resume, pause/resume/cancel, events | **complete** (session 2) — cargo gates and the DoD download run on the Windows machine |
| M2 | Queue + SQLite + scheduler + limiters | pending |
| M3 | Full UI wired to engine (both list modes, add dialog, settings, fuzzy search) | pending |
| M4 | Vault: cookies, proxy, custom headers per task | pending |
| M5 | Tray, notifications, shutdown-after-queue, boost mode, clipboard catcher | pending |
| M6 | MV3 extension + native messaging host + pairing | pending |
| M7 | FTP engine, checksums, task import/export | pending |
| M8 | Polish: light theme, error hardening, NSIS installer, README, final review | pending |

### Known seams (by design, not loose ends)
- `StoreProvider.loadTasks()` returns an empty list — the real read path attaches the
  SQLite store at M2; the state machine around it is already final.
- `TasksView` returns `null` for a non-empty task list; row rendering attaches at M3
  when the engine feeds live tasks. Every state reachable today is fully rendered.
- Sidebar counts are real computations over the (genuinely empty) task list — they
  become live the moment M2/M3 deliver tasks, with no wiring changes.
- The engine and CLI are standalone; nothing in `src/` or `src-tauri/` calls
  `spawn_download` yet (fence for M1). The desktop host attaches at M2/M3.
- `Cargo.lock` appears with the first `cargo build` on Windows and is committed then.
- The sandbox has no Rust compiler, so the M1 Rust code is authored against stable
  APIs without a local compile; the first Windows run is expected to apply rustfmt
  normalization and to be the proving run for clippy/tests (any findings are fixed
  before the milestone is called done on the user side).

---

## 8. Session log

### Session 1 — M0 scaffold
Commits: `chore: initialize repository tooling…` → `feat(ui): dark app shell…` →
`feat(tauri): Tauri 2 host…` → this document.

What landed: full shell per the design spec (sidebar with three sections, count
badges, collapse with persistence, top bar with title/count/search/refresh,
skeleton/empty/no-match states), strict TypeScript + type-aware ESLint + content gate,
Tauri host with least-privilege capability, dual CSP, original icon set, and the
`app_version` IPC round trip.

Decisions worth knowing:
- One strict tsconfig instead of a split app/node pair — the type-aware lint project
  service resolves a single root project reliably.
- The CSP split (`csp` vs `devCsp`) exists because release builds must forbid inline
  scripts while React Refresh needs them in development.
- lucide's current icon set uses `CircleCheckBig` and `Disc3` (its `Music` mark was
  retired) — check installed exports before referencing icons.
- Repo-wide content rules are enforced by a scanner rather than discipline alone.

### Session 2 — M1 engine core
Commits: `chore: add cargo workspace…` → `feat(engine): manifest, workspace,
segmenter, and event core` → `feat(engine): segmented HTTP downloads…` →
`feat(cli): rapids CLI…` → `test(cli): kill-and-resume round trip…` → this document.

What landed: a cargo workspace (`crates/*` + `src-tauri`), the full engine library
(probe, segmented fetch with retries/backoff/cancel, manifest + workspace locking,
scaling, SHA-256 finalize), the `rapids get/cancel` CLI with live progress and
signal handling, a deterministic in-repo HTTP test server, unit + integration
tests, and the 100 MB kill-at-50% → resume → hash acceptance test.

Decisions worth knowing:
- Dynamic scaling is a pure function of measured windows: baseline at the start
  count, then add one connection per 5s window only while the relative gain ≥ 20%,
  stop permanently once it falls below; a pinned `--connections` turns scaling off.
- Resume requires three independent proofs (range support recorded, size matches
  the fresh probe *and* the part file's length, validator matches) — anything else
  wipes and restarts, because splicing would be worse than re-downloading.
- The destination name resolves after the probe (`-o` → server filename → URL
  segment), so the workspace/lock form around the final path; explicit `-o` runs
  settle a previously interrupted finalization before probing (works offline).
- Pausing joins workers before the Paused state is announced and re-queues from
  flushed progress; a resumed session starts with fresh flags and a drained
  message channel so no stale worker message leaks across sessions.
- Locks are operating-system file locks, not marker files: a killed process
  releases them by itself, which is what makes the kill-and-resume test possible.
- Control wakes use `watch` with change-only notifications; the body pump selects
  between the next chunk and the control channel, so pause/cancel land within one
  chunk of network time and every network call has a timeout.
- Tests throttle real sockets instead of mocking: timing behaviour (pause points,
  kill points, retries) is exercised against bytes actually in flight.

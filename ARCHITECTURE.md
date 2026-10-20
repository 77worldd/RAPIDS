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
| Async runtime | tokio (`rt-multi-thread`, `macros`) | 1.x |
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
  segmenter, rate-limiter, protocol clients, vault) attach here with their milestones.
- **`commands.rs`** serves `app_version` from `package_info()` — the product name and
  version have one source of truth: `tauri.conf.json`.

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
| Rust format | `cargo fmt --all -- --check` | Windows machine (see below) |
| Rust lints | `cargo clippy --all-targets -- -D warnings` | Windows machine (see below) |
| Rust tests | `cargo test` | Windows machine (see below) |

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
| M0 | Scaffold: Tauri 2 + React + TS + Tailwind shell per design | **complete** (this session) |
| M1 | Engine core: segmented HTTP, kill-safe resume, pause/resume/cancel, events | pending |
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
- `Cargo.lock` appears with the first `cargo build` on Windows and is committed then.

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

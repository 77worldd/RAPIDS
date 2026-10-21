# RAPIDS — Architecture

RAPIDS is a Windows desktop HTTP(S) download manager. This is the implemented
architecture at M2's fenced tip, `2433b6444198d8353a43dfa96d9ae14cb56aca7b`.
The session log below preserves earlier snapshots; the current map and milestone
status in this document supersede their historical pending notes.

**Implemented scope:** persisted download tasks, an ordered queue, timed start
windows, retry/backoff, rate and connection limits, and a desktop task view.
BitTorrent, HLS/M3U8, and media-site parsing are outside the implemented scope.

## 1. Stack

| Layer | Implemented technology |
| --- | --- |
| Desktop shell | Tauri 2; Rust commands and targeted events |
| Interface | React 18, TypeScript, Vite, Tailwind CSS 4, Framer Motion, lucide-react |
| Transfer core | Rust, Tokio, reqwest with rustls, SHA-256 |
| Persistent host | SQLite via sqlx 0.8, versioned SQL migration |
| Interchange | Serde JSON for IPC, settings values, and sidecar manifests |
| CLI | clap-based `rapids` binary for headless transfers |

The Cargo workspace contains `rapids-engine`, `rapids-host`, `rapids-cli`,
`rapids-test-support`, and the `src-tauri` app. The web interface is a
Vite/React application at the repository root.

## 2. Repository map

```text
RAPIDS/
├── Cargo.toml / Cargo.lock       Rust workspace and locked dependencies
├── package.json / package-lock.json
├── crates/
│   ├── rapids-engine/
│   │   ├── src/{config,connections,coordinator,error,events,hash,lib,
│   │   │         manifest,rate,segmenter,util,workspace}.rs
│   │   ├── src/http/{fetch,mod,probe}.rs
│   │   └── tests/{connections_test,download_test,manifest_test,
│   │             rate_test,segmenter_test}.rs
│   ├── rapids-host/
│   │   ├── migrations/0001_init.sql
│   │   ├── src/{api,error,events,lib,manager,progress,runner,scheduler,wire}.rs
│   │   ├── src/store/{db,history,mod,schedules,settings,tasks}.rs
│   │   └── tests/{api_test,manager_test,scheduler_test,store_test}.rs
│   ├── rapids-cli/src/{cli,main,render}.rs
│   ├── rapids-cli/tests/kill_resume.rs
│   └── rapids-test-support/src/lib.rs
├── src-tauri/src/{adapter,commands,lib,main,platform}.rs
├── src/
│   ├── {App,main}.tsx / types.ts / index.css
│   ├── components/{layout,motion,sidebar,tasks,topbar,ui}/
│   ├── config/{categories,navigation}.ts
│   ├── lib/{addDraft,cn,format,ipc,uiPrefs}.ts
│   ├── state/StoreProvider.tsx / selectors.ts
│   └── views/TasksView.tsx
├── scripts/content-gate.mjs
└── tests/add-draft.test.mjs
```

`rapids-engine` and `rapids-host` are headless: no webview or Tauri type
crosses into them. The Tauri crate supplies platform paths, runs the host,
exposes commands, and forwards host events. The CLI and test-support crate
exercise the same engine independently of the desktop shell.

## 3. Module responsibilities

### Transfer core — `crates/rapids-engine`

- `http/probe` identifies size, byte-range support, validators, and HTTP
  metadata; `config` validates transfer parameters and timeouts.
- `coordinator` chooses a fresh or resumable run, drives `segmenter` and
  `http/fetch`, handles retry/stop and checkpoints progress. `manifest` owns
  the resume record and its reuse gate; `workspace` owns the sidecar and OS
  lock; `hash` checks the result. `events` defines the engine event contract.
- `connections` holds an app-wide lease budget around active HTTP attempts,
  including probes; parked workers and backoff do not hold a lease. `rate`
  shares an app-wide byte budget and combines it with per-task ceilings.
- The coordinator verifies completed bytes before publishing the final file
  by rename; in-flight bytes remain in `data.part`. The engine reports whether
  a failure is retryable and preserves the original error message.

### Host — `crates/rapids-host`

- `store/db` opens SQLite, enables WAL and foreign keys, and applies the
  migration. `store/{tasks,schedules,history,settings}` owns typed persistence;
  the queue is a projection of waiting task rows, not a second queue table.
- `api` validates operations on tasks, queue, settings, and schedules and
  returns authoritative views. `wire` parses camelCase desktop requests and
  selects one event channel per event kind.
- `manager` serializes task/queue transitions, orders and starts eligible
  work, tracks active runners, and recovers interrupted tasks at startup.
  `runner` bridges engine reports and stop handles to the manager. `scheduler`
  handles once/daily/weekly windows and retry deadlines. `store/settings`
  reads typed values; `api` validates updates and `manager` applies live
  limits with bounds.
- `events` defines `TaskView`, `ProgressUpdate`, and `HostEvent`; `api` defines
  `QueueCounts`. `progress` coalesces at most 64 task samples per batch over
  a 500 ms window. The `EventSink` contract is `Send + Sync`; status and queue
  changes are discrete events.

### Desktop — `src-tauri` and `src`

- `src-tauri/adapter` opens the host under the platform-selected data path and
  emits host events; `platform` supplies paths and clock integration;
  `commands` maps Tauri IPC calls to the headless host. The shell owns its
  lifetime and stops it cleanly on exit.
- `src/lib/ipc.ts` is the typed IPC/event boundary and checks host replies.
  `StoreProvider` hydrates from SQLite-backed host views, applies validated
  events, and owns filter/search and queue UI state. `uiPrefs` stores only
  sidebar collapse locally.
- `TasksView`, `components/tasks`, `components/topbar`, and
  `components/sidebar` render persisted tasks, category/status views, search,
  Add Download, queue pause/resume, refresh, and applicable task actions.
  Progress and completion come from the host, not client-side simulation.

### Headless tools

- `rapids-cli` exposes direct get/cancel commands without the Tauri shell;
  its `kill_resume` test exercises killed-process recovery. `rapids-test-support`
  supplies deterministic HTTP fixtures for engine/host/CLI integration tests.

## 4. Stored data and wire contracts

| Contract | Implemented representation and ownership |
| --- | --- |
| SQLite | The shell uses the platform local-data path under `RAPIDS/rapids.db` (on Windows, `%LOCALAPPDATA%\RAPIDS\rapids.db`). Migration `0001_init.sql` defines `tasks`, `schedules`, `history`, and `settings`. |
| Task state | `queued`, `scheduled`, `downloading`, `paused`, `completed`, `error` in SQLite. Higher priority runs first; an arrival sequence breaks ties. Categories: `apps`, `movies`, `music`, `images`, `other`. History survives deletion of a task row. |
| Settings | Database keys are snake_case; values are JSON scalars. Seeded defaults: `max_parallel=3`, `max_connections_total=48`, `global_speed_limit=null`, `resume_on_start=true`, `retry_max_attempts=3`, `retry_base_ms=5000`, `retry_cap_ms=300000`, `default_dest_dir=""`. The typed IPC `Settings` view uses camelCase. |
| Schedule | `once`, `daily`, `weekly`, with `start_at` in epoch milliseconds; a weekly weekday mask uses Monday bit 0 through Sunday bit 6. `window_minutes=0` has no closing deadline for running work. |
| Sidecar | For destination `<dest>`, the engine stores `<dest>.rapids/{manifest.json,data.part,lock}`. The JSON manifest records URL, total size, range support, validators, and each segment's committed offset; `data.part` is sized to the known total, so its length alone is not a progress measure. SQLite tracks display/queue state, not which bytes are safe to reuse. |
| Resume authority | Saved bytes are eligible only if the manifest recorded range support, its total size matches the fresh probe, the stored ETag matches the fresh ETag (or, without a stored ETag, stored Last-Modified matches fresh Last-Modified), and `data.part` has the fresh size. Otherwise the engine starts a fresh transfer. Requeuing an interrupted task is not byte-level authorization. |
| IPC/events | Commands validate requests and return camelCase task/schedule/settings views. `HostEvent` is a `kind`-tagged union on `rapids://progress`, `rapids://task-changed`, `rapids://task-removed`, and `rapids://queue-changed`. The frontend validates incoming payloads. |
| Local UI preference | `localStorage["rapids.ui"]` holds a version-1 JSON object with `sidebarCollapsed`; it is not download state. |

The host seeds `resume_on_start=true`. On startup with that setting enabled,
`manager::recover` requeues persisted `downloading` rows, then pumps the
queue. The manifest/part-file gate above independently decides whether their
bytes can be resumed. README.md describes the user-facing policy.

## 5. Interface conventions

The interface uses a dark neutral palette, an emerald action accent, compact
navigation, and host-backed status and progress. Task rows show actionable
controls appropriate to their actual state; search and category/status views
filter the hydrated host task list. The Add Download form submits a real
host request. Loading and empty states do not fabricate task results.

## 6. Verification

- Frontend: `npm ci`, `npm run gate:content`, `npm run lint`,
  `npm run typecheck`, `npm run test:add-draft`, and `npm run build`.
  `scripts/content-gate.mjs` rejects task markers, staged-feature phrasing,
  invented-data markers, and filler wording.
- Rust: `cargo fmt --all -- --check`, build, clippy with warnings denied,
  and tests against the crates supported by the verification host. Linux
  headless checks use the four `crates/` packages; the desktop crate needs
  native GTK/WebKit libraries not present in that environment. The Windows
  fence can build and test the complete workspace.
- The CLI's standalone `cargo test -p rapids-cli --test kill_resume --locked`
  covers a real killed process and byte-exact restart. It is a required
  separate check after engine-touching commits.
- At `2433b64`, independent Linux verification passed formatting, build,
  clippy, frontend gates, and 118/0 headless tests; the Windows warm rerun
  passed 119/0 workspace tests, the standalone kill/resume test, and the
  desktop smoke check (details in Session 4 below).

## 7. Milestones

| Milestone | Status |
| --- | --- |
| M0 — shell foundation | complete |
| M1 — HTTP(S) engine and CLI | complete |
| M2 — persistent queue, host, scheduler, limits, and live task interface | **closed 2026-10-21** at `2433b64` |
| M3 | authorized, planning in progress |

## 8. Session log

Sessions 1–3 below are historical notes written before M2 closed. References
to work pending or formatting debt describe those earlier checkpoints, not
this document's current status.

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

### Session 3 — M1 finish: compile repair and an honest acceptance test
Commits: `fix(engine): repair http worker and probe compile errors` →
`fix(engine): repair coordinator borrow, control, and client plumbing` →
`test(cli): make the kill-and-resume acceptance test observable` →
`fix(cli): bind the cancel target as mutable` →
`refactor(engine): scope the probe future so its borrows end early` →
`test(cli): reuse one manifest reader runtime across the kill poll` → this document.

What landed: every compile error from the independent audit is closed, plus the ones
that only appear once the never-compiled test and CLI sources are read with the
compiler's rules in mind. No module was split or redesigned. Frontend gates stayed
green (`gate:content`, `lint`, `typecheck`, `build`).

Fixes, by file:
- `http/fetch.rs` — the retry jitter seed widens the segment index before rotating it
  (`u64`, not `u32`); the `Range` header and the short-read retry message bind their
  numbers as locals, because a format string cannot capture a field access; the worker
  started message dropped its unused payload, which `-D warnings` would have rejected
  as a dead field.
- `http/probe.rs` — the HEAD outcome became one tuple write instead of two variables
  that were always overwritten (the dead-store warning class), and the
  `Content-Disposition` filename resolves through `?` so the option is unwrapped before
  the name is sanitized.
- `coordinator.rs` — the probe's HTTP client now travels through `Prepared` instead of
  being rebuilt (one pool, one set of timeouts); the boxed probe future lives in its own
  block so the shared borrow of `self.spec` ends before the destination branch writes to
  it; `cancel_now` takes `Option<&mut Workspace>` and both call sites pass it, so a
  cancel reaches the sidecar; the session control receiver is hoisted out of the pause
  branch; every checkpoint call passes its directory; `AsyncWriteExt` is imported for the
  part-file flush; the previous scaling window is an `Option<f64>` instead of a seeded
  float plus a flag.
- `crates/rapids-cli` — `rapids cancel` binds its workspace mutably (locking and
  discarding both take a mutable receiver).

Behaviour worth knowing:
- Lifting a pause announces `Downloading` before the next session starts. The handle's
  `resume()` waits for exactly that transition, so without it a resume would wait for an
  event that never came even though bytes were flowing.
- The kill-and-resume test no longer observes the part file's length: `data.part` is
  preallocated to the final size at start, so the old check fired before the first byte
  arrived and made the "must not refetch everything" assertion impossible. The test now
  sums the manifest's `written` values, which is the same conservative low-water mark the
  engine resumes from, and kills at half of the payload on that basis. Two `std::sleep`
  calls the audit never reached were `std::thread::sleep`.

Status: M1 is code complete and awaiting the proving run on the owner's machine —
`cargo build --workspace`, `cargo fmt --all -- --check`,
`cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, and
the acceptance test standalone. `Cargo.lock` is committed with that first resolution.

### Session 3, second pass — independent verification and repairs
Session 3's own code was compiled and run on a machine with a real toolchain and
registry access. The engine's logic held up; the build was not green. Everything it
reported is repaired, with the engine's architecture unchanged.

- Engine: one binding had to be mutable — flushing a part file takes `&mut self` once
  `AsyncWriteExt` is in scope.
- `rapids-test-support` had never been compiled by anything, because every earlier
  build stopped inside the engine library first. Nine errors, all mechanical: header
  values captured by field access in format strings, a string literal asked for
  `.into_bytes()`, `u64 % u32`, and a `Cow` returned where `Option<String>` was
  declared. HEAD and a range-less GET now share one full-body response builder, so
  that header block exists once instead of twice.
- Clippy findings (nine): the done and failed worker messages carried a never-read
  segment index, two module descriptions were `///` above a blank line instead of `//!`,
  the URL filename fallback took the last segment with `filter().last()` instead of
  `next_back()`, the backoff multiplication converted a value that was already `u128`,
  the request target was bound and never read, and the flaky counter compared a
  remainder to zero instead of using `is_multiple_of`.
- Kill-and-resume accounting: the engine's resume was proven correct on that machine —
  the second run requested only the unconfirmed remainders and the final file hashed
  identically. The failure was in the test's accounting: the server pushes a record
  when a connection ends, so the killed process's last eight records landed after
  `clear_requests` and were counted against the resumed run. The server now counts
  in-flight connections and exposes `wait_idle`; the test waits for it before clearing
  the log. The per-connection throttle also drops to 2 MB/s, which moves the kill three
  seconds clear of natural completion instead of sixty milliseconds.
- Validator-change test: the premise could never happen. The server snapshots its
  options when a connection starts and the engine keeps one long-lived connection per
  segment, so with four segments nothing sent another request after the validator
  flipped, and the task completed on consistent bytes — correct behaviour from the
  engine, a test that asserted nothing. Early drops now force a post-flip retry that
  carries `If-Range: "v1"` into a server serving `"v2"`, which is exactly the refusal
  the test exists to check.

Still pending: `cargo fmt --all` normalization (the Rust side was authored without
rustfmt, so this is the one expected red gate) and `Cargo.lock`, both handled by the
owner's Windows run.

### Session 3, third pass — the last two findings
The second independent run (real toolchain, full registry) was green on build, on the
whole test suite bar one assertion, and on the 100 MB acceptance test: 28 tests, with
the kill-and-resume lifecycle visible end to end in 12.7 s and the final file matching
the source hash. Two findings remained.

- `filter_next` (clippy 1.99): `filename_from_url` walked the path segments with
  `.filter(|segment| !segment.is_empty())` and then `.next_back()`. Clippy asks for
  `.rfind()`, which is the same traversal expressed as one call, so the URL filename
  fallback is now `parsed.path_segments()?.rfind(...)`.
- A test contradicted the specification. `resume_requires_matching_validator_and_size`
  expected `can_resume` to accept a fresh probe that drops a stored ETag as long as
  Last-Modified still matched. That is not the gate: the stored ETag is the preferred
  validator, so when the manifest holds one the fresh response must re-present it, and
  a swapped validator means the two responses are no longer comparable — the gate wipes
  and restarts. The implementation was already correct and is unchanged; the assertion
  now states the rule (`assert!(!…, None, Some(<matching date>))`) with the reasoning
  next to it. The sibling case — a stored manifest with no ETag resuming on a matching
  Last-Modified — is the documented fallback and still asserts `true`.
  The assertion had never executed before, which is how a spec-violating expectation
  survived review.

With these two applied the engine side is complete on the verification toolchain:
build clean, clippy `-D warnings` clean across all three crates and all targets, 28/28
tests. rustfmt normalization and `Cargo.lock` remain with the Windows battery.

### Session 4 — M2 close (2026-10-21)

- The implemented M2 stack separates the headless SQLite host from the thin
  Tauri adapter and live task interface. The manager owns ordered work,
  schedules, retries, settings-driven app-wide limits, and startup recovery;
  the engine owns network transfers, sidecar checkpoints, and byte reuse.
  `resume_on_start` is seeded `true`: interrupted `downloading` rows requeue,
  but only the engine's recorded-range/size/validator/part-length gate can
  reuse their bytes.
- The final M2 code and test-only rate-band correction were fenced at
  `2433b6444198d8353a43dfa96d9ae14cb56aca7b`. Independent Linux
  verification passed 118/0 headless tests, plus build, clippy, formatting,
  and frontend checks. No exact aggregate throughput measurement was
  reported by the passing rate test.
- The operator's Windows warm rerun on that clean tip passed the full
  workspace suite (119 passed, 0 failed, 0 ignored), the standalone
  `kill_resume` test (1 passed; test time 14.50 s), and five of five Tauri
  smoke checks: window launch, empty-list/no-service-error state, queue
  pause/resume, a real 2,097,152-byte download with byte-exact SHA-256, and
  persistence of two completed tasks and their hashes after restart. Source
  and completed file SHA-256 matched:
  `5647F05EC18958947D32874EEB788FA396A05D0BAB7C1B71F112CEB7E9B31EEE`.
- The verifier declared **M2 closed** at `2433b64`. This documentation
  update records that closure; it does not change product behavior.

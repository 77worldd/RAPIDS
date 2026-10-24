# Ezoo parity working checklist

Reference benchmark: `AhmedEzo686/ezoo-download` (all rights reserved). It is read as a
specification only; no source, strings, or artwork are copied. Every item below is
re-implemented in RAPIDS idioms.

Status legend: **[x]** complete, **[~]** in progress, **[ ]** queued, **[–]** deferred with reason.

## Phase A — brand palette

- [x] `src/index.css` accent tokens: `#1FB9DF`, `#62E4F4`, `#0C7CA4`, `#F97316`, `#FBAE3C`.
- [x] Ink-tinted surfaces: canvas, panel, elevated, sidebar, line, muted, high.
- [x] Primary button, toggle, focus rings, drag ring, selection and body glow.
- [x] Task statuses: downloading cyan-core, completed cyan-deep, queued sky, scheduled violet,
      paused amber, failed red.
- [x] Category tones: archives signal-orange, documents beak-light, apps cyan.
- [x] Extension badge and popup/options palette moved to the brand accent.
- [x] Regression suite `tests/theme.test.mjs` proves zero stock green in `src/`.

## Phase B — engine

- [ ] Per-download request headers (referer, cookies, user agent, basic auth, extras) through
      probe, transfer, retries, persistence, and the resume gate.
- [ ] Probe fallback ladder on 403/406/416/429 with the winning header set recorded and reused.
- [ ] HLS: master playlist variant choice, AES-128 keys, init map, part downloads with resume,
      concatenation, live-stream and foreign-DRM refusal.
- [ ] Tools manager: runtime download of yt-dlp and ffmpeg into the local app data folder,
      daily update stamp, replace-while-running safety.
- [ ] Link resolution through yt-dlp metadata only, with a quality menu and re-resolve by key.
- [ ] Mux through ffmpeg stream copy with container-appropriate output.
- [ ] Rolling quota window with automatic pause and resume.
- [ ] Free-space and FAT32 size checks before start.
- [ ] Per-server connection overrides.
- [ ] Local range fixtures: slow first eighth, byte-identical pause/resume, 32 connections;
      plus parser and option-builder unit tests.

## Phase C — host and settings

- [ ] Queues table with non-deletable Main queue, per-queue limits, folder, retries, after-done.
- [ ] Persisted queue pause state and move-to-queue.
- [ ] After-done actions with a cancellable power countdown.
- [ ] History list, purge, and re-download commands.
- [ ] Unfinished-task edit and finished-file move/rename commands.
- [ ] Settings keys for clipboard, tray, autostart, sounds, category folders, duplicate policy,
      notifications, default user agent, quota, limiter-at-start, extension port, server
      exceptions, and site logins.
- [ ] Scheduler windows gain a stop time and queue binding.

## Phase D — frontend

- [ ] Download File Info dialog with live probe, category, save-as, auth, connections, queue.
- [ ] Batch add: link list and numeric/letter pattern tabs.
- [ ] `.ef2` and plain-text import/export.
- [ ] Site Grabber: templates, depth, scope switches, progress, results checklist.
- [ ] Playlist: flat listing, quality, parallel items, numbered names, auto-created queue.
- [ ] Queues UI: sidebar section, editor, start/stop, move-to-queue.
- [ ] Scheduler UI bound to queues.
- [ ] History view: open folder, copy URL, re-download.
- [ ] Properties dialog including the observed SHA-256.
- [ ] Toolbar and status parity adapted to the existing top bar.
- [ ] Completion notification, optional sound, and completion card.
- [ ] Browser-integration helper.
- [ ] Tray menu and close-to-tray using the locked tray artwork.
- [ ] Autostart setting through `tauri-plugin-autostart`.

## Phase E — extension v2

- [ ] Localhost JSON API on `127.0.0.1` with origin gating, a capability header, POST-only
      routes, and size caps: `/ping`, `/config`, `/add`, `/links`.
- [ ] Deep link remains the fallback when the app is unreachable.
- [ ] Extension settings sync through `/config`.

## Deferred

- [–] i18n: eight locales need a translation workflow and review capacity that this branch
      does not carry.
- [–] Self-update: release signing and a hosted manifest are outside this branch.
- [–] Light theme and theme gallery: the brand palette ships dark-first; a full light ramp needs
      a separate contrast pass.
- [–] Window opacity: compositor-dependent and not part of the parity scope.
- [–] FTP/SFTP: engine scope is HTTP, HLS, and CLI-resolved media.
- [–] Torrent/magnet: needs a BitTorrent stack rather than the HTTP engine.
- [–] Signed Firefox xpi: signing requires an AMO account and key material.
- [–] Virus-scan hook: needs a chosen scanner contract and user configuration.
- [–] Speed graph: the task row already reports live speed and ETA; a graph is presentation-only.
- [–] Per-file scheduling: queue-level windows cover the parity requirement.
- [–] Encrypted credential storage: site logins stay in the local store; OS keychain integration
      is a separate security project.

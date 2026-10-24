# RAPIDS — Logo & Brand Integration Prompt (Arena AI)

You are working on **RAPIDS**, a Tauri 2 desktop download manager (`com.rapids.desktop`, React + TypeScript frontend in `src/`, Rust backend in `src-tauri/`, Windows-first, NSIS bundle).
The logo has been finalized and a complete, pre-rendered asset kit ships with this prompt. Your job is to **wire that logo into every surface of the app**: app icon (taskbar / title bar / exe / installer), tray, frontend UI, browser extension, and docs — with zero breakage of existing features.

---

## How the assets reach you

You have no access to the designer's machine — the kit arrives **through the repo**. Check in this order and follow the first path that applies:

- **Path A — kit already in repo (expected):** `brand/` exists at the repo root with subfolders `master/`, `tauri-icons/`, `tray/`, `extension/`, `web/` plus `ARENA-PROMPT-logo-integration.md` and `KIT-SHEET.png` (36 files total — uploaded beforehand by a separate upload task). Verify with `ls brand/` (or `Get-ChildItem brand`), then continue; Task 1 reduces to committing anything still untracked.
- **Path B — zip staged in repo root:** `rapids-brand-kit.zip` is present. Extract it so the kit lands at `brand/`:
  - PowerShell: `Expand-Archive rapids-brand-kit.zip -DestinationPath . ; Move-Item rapids-brand-kit brand`
  - bash: `unzip -q rapids-brand-kit.zip && mv rapids-brand-kit brand`
- **Path C — neither found:** STOP. Tell the user: *"brand kit not found in the repo — upload `rapids-brand-kit.zip` (see the upload instructions that came with this prompt), then re-run me."* Never invent or substitute placeholder icons, never generate the artwork yourself.

---

## 0. Operating rules

1. **This is a branding task, not a refactor.** Do not touch download engine, sniffing, deep-link, rate-limiter, or task-queue logic.
2. **Do NOT change the app's existing accent colors.** The UI currently uses green accents; removing them is a *separate, future task*. You only ADD the logo assets. (The logo palette is cyan/orange on ink — it is locked. See `brand/master/PALETTE.md`.)
3. Never edit the artwork. All SVG/PNG files are locked masters generated from final vector curves. If a size is missing, render *down* from `brand/master/` files — never redraw, never "improve" paths.
4. Every step below says **Verify first**. Run the verification before changing anything, and skip gracefully (with a note in your report) if the target does not exist.
5. Work on a new branch: `git checkout -b brand/logo-integration`.
6. Commit in small logical commits, e.g. `feat(brand): replace app icons with v4 mark`, `feat(brand): wire tray icon`, `feat(brand): add logo to UI header`.

---

## 1. Step zero — context verification

Confirm each of the following before starting; adjust paths if the repo differs, and note deviations in your report:

- [ ] Repo root contains `src-tauri/tauri.conf.json` with identifier `com.rapids.desktop`.
- [ ] `src-tauri/icons/` exists with the default Tauri icon set (`32x32.png`, `128x128.png`, `128x128@2x.png`, `icon.ico`, `icon.icns`, `icon.png`).
- [ ] Frontend entry is `index.html` + `src/` (React). Grep for template leftovers: `tauri.svg`, `react.svg`, `vite.svg` — these are likely placeholder images/logo references to replace.
- [ ] Search for existing tray code: `rg -n "TrayIcon|TrayIconBuilder|tray" src-tauri/src/`.
- [ ] Search for a browser extension in the repo: `rg --files -g manifest.json` (MV3 extension, may or may not exist yet — it is built by a separate task).
- [ ] `npm run build` passes BEFORE your changes (baseline).
- [ ] The `brand-kit/` folder provided alongside this prompt is available (copy it into the repo root as `brand/` first — see Task 1).

---

## 2. Asset kit manifest

The kit is laid out exactly as follows. Nothing here was hand-drawn at small sizes; everything is rendered from the two master SVGs.

```
brand/
├── master/
│   ├── app-icon.png              # 1024×1024 squircle icon, transparent corners — SOURCE for `tauri icon`
│   ├── rapids-mark.svg           # vector app icon (bird on ink squircle tile)
│   ├── rapids-glyph.svg          # vector bird glyph, transparent background
│   ├── glyph-1024.png / glyph-2048.png
│   ├── glyph-mono-white-1024.png # single-color white glyph (dark surfaces)
│   ├── glyph-mono-ink-1024.png   # single-color ink glyph (light surfaces)
│   └── PALETTE.md                # locked color tokens
├── tauri-icons/                  # drop-in replacement for src-tauri/icons/ (desktop + MSIX set)
│   ├── 32x32.png  128x128.png  128x128@2x.png  icon.png(512)  icon.ico(16→256, 9 frames)
│   └── Square30x30Logo.png … Square310x310Logo.png  StoreLogo.png(100)
├── tray/
│   ├── tray-32.png               # colored glyph, transparent bg (default tray icon)
│   ├── tray-16.png
│   ├── tray-32-mono.png          # white  (dark taskbars)
│   └── tray-32-mono-dark.png     # ink    (light taskbars)
├── extension/
│   ├── icon-16.png  icon-48.png  icon-128.png   # transparent glyph, MV3-ready
├── web/
│   ├── favicon.ico (16/32/48)  favicon-32.png  apple-touch-icon.png  logo-mark.svg
└── KIT-SHEET.png                 # proof sheet of every generated asset
```

Locked palette (do not deviate): `cyan-core #1FB9DF` (primary), `cyan-light #62E4F4`, `cyan-deep #0C7CA4`, `signal-orange #F97316`, `beak-light #FBAE3C`, `ink-bg #17293F`, `white #F4F9FC`.

---

## 3. Task 1 — install the kit in-repo

- If the kit arrived via **Path A/B** ("How the assets reach you"), it is already at `<repo>/brand/` — just make sure it is committed. Only if you were handed the kit out-of-band, copy it to `<repo>/brand/` (yes, commit the SVGs and PNGs — they are small and are the canonical source).
- `git add brand/ && git commit -m "feat(brand): add v4 logo asset kit"` (skip if the upload task already committed it and the tree is clean).
- Do not duplicate masters into other folders; everything else must *reference or copy from* `brand/`.

## 4. Task 2 — app icon: taskbar, title bar, exe, installer

**Verify first:** list `src-tauri/icons/` and read `bundle.icon` in `src-tauri/tauri.conf.json`.

1. Regenerate the full official icon set from the master (this also produces `icon.icns`, which the kit intentionally does not include):

   ```bash
   npx tauri icon brand/master/app-icon.png --output src-tauri/icons
   ```

   This overwrites/adds the desktop set (`32x32.png`, `128x128.png`, `128x128@2x.png`, `icon.ico`, `icon.icns`, `icon.png`) plus the MSIX `Square*Logo.png`/`StoreLogo.png` set (and mobile folders if the project has mobile init — leave them as generated).
2. Cross-check: the regenerated files must be visually identical to `brand/tauri-icons/*` (same source). If `npx tauri icon` fails in your environment, copy `brand/tauri-icons/*` over `src-tauri/icons/` instead and note that `icon.icns` was left unchanged.
3. Confirm `tauri.conf.json` → `bundle.icon` lists (relative to `src-tauri/`):
   `icons/32x32.png`, `icons/128x128.png`, `icons/128x128@2x.png`, `icons/icon.icns`, `icons/icon.ico`.
4. Windows behavior to rely on (no code needed): the window title-bar icon, taskbar icon, exe icon, and NSIS installer icon all derive from the bundled `icon.ico`. Your duty is to make sure the bundle step succeeds and the files are in place.

**Verify after:** run the icon-audit script in §9; then `npm run tauri build` (or at minimum `npx tauri build --debug --no-bundle`) and confirm the exe carries the new icon (Explorer → check `src-tauri/target/release/rapids.exe` — name may differ; look for the produced binary).

## 5. Task 3 — tray icon (only if tray already exists)

**Verify first:** `rg -n "TrayIcon|TrayIconBuilder|SystemTray" src-tauri/src/ src-tauri/Cargo.toml` and check whether `tray-icon` feature is enabled in the tauri dependency.

- **If tray code exists:** copy `brand/tray/tray-32.png`, `tray-16.png`, `tray-32-mono.png`, `tray-32-mono-dark.png` into `src-tauri/icons/`, then replace whatever image the builder uses with the new asset, e.g.:

  ```rust
  let tray = TrayIconBuilder::with_id("main")
      .icon(tauri::include_image!("icons/tray-32.png")) // embeds at compile time
      // ... keep all existing menu/tooltip/event wiring unchanged
  ```

  - Keep `icon_as_template(true)` only on macOS logic if it already exists.
  - Do not add new tray menus/behaviors.
- **If no tray exists:** do NOT invent one. Note in your report: "no tray in app; tray assets staged in `brand/tray/` for a future tray task."

## 6. Task 4 — frontend branding (React UI)

**Verify first:** read `index.html`, the main layout component (search `src/` for the app shell / header), and grep for `tauri.svg|react.svg|vite.svg|logo`.

1. `index.html`:
   - Set `<link rel="icon" type="image/png" href="/favicon-32.png" />` (put `brand/web/favicon-32.png` into `public/`), plus `<link rel="apple-touch-icon" href="/apple-touch-icon.png" />`.
   - Copy `brand/web/favicon.ico` to `public/favicon.ico`.
   - Confirm `<title>` reads `RAPIDS` (fix if it still says Tauri + Vite template text).
2. App header / sidebar: add the logo lockup at the top of the main navigation or title bar area:
   - Copy `brand/web/logo-mark.svg` to `src/assets/rapids-mark.svg`.
   - Render it at ~28–32 px height next to the wordmark text `RAPIDS` (styled with the app's existing heading font/color — do not introduce new UI accent colors).
   - Remove/replace any template placeholder image it displaces (`react.svg`, `tauri.svg`, etc.), including unused imports and files if nothing else references them.
3. About / settings section (if one exists): show the app icon (`brand/tauri-icons/128x128.png` → copy to `public/` or import) + version string. If there is no About section, skip.
4. Loading/splash state (if the app has one): use `logo-mark.svg` or `glyph-mono-white-1024.png` on ink background `#17293F`. Skip if none.
5. TypeScript cleanliness: any SVG import must compile (`npm run build` gate). If the template lacks an SVG module declaration, add `src/vite-env.d.ts` reference or `declare module '*.svg'` as appropriate — smallest change that makes the build pass.

## 7. Task 5 — browser extension icons (conditional)

**Verify first:** `rg --files -g manifest.json -g '!node_modules'`. The MV3 extension from the separate extension task may or may not be merged yet.

- **If the extension exists** (a `manifest.json` with `"manifest_version": 3`):
  - Copy `brand/extension/icon-16.png`, `icon-48.png`, `icon-128.png` into the extension folder (e.g. `extension/icons/`).
  - Wire the manifest:

    ```json
    "icons": { "16": "icons/icon-16.png", "48": "icons/icon-48.png", "128": "icons/icon-128.png" },
    "action": { "default_icon": { "16": "icons/icon-16.png", "48": "icons/icon-48.png" } }
    ```

  - If the popup HTML exists, add the same logo at the top of the popup header (`logo-mark.svg` copied into the extension, ~24 px height + `RAPIDS` text).
- **If it does not exist yet:** create `extension-brand/README.md` stating the icons are staged in `brand/extension/` (16/48/128) and that the extension task should consume them. Do not scaffold a whole extension here.

## 8. Task 6 — README / docs

- In `README.md`, add a centered header at the very top:

  ```markdown
  <p align="center"><img src="brand/tauri-icons/128x128.png" width="96" alt="RAPIDS logo" /></p>
  <h1 align="center">RAPIDS</h1>
  ```

  (or the repo's existing badge style if headers already exist — integrate, don't clobber).
- If `docs/` exists with an index/landing page, use `brand/master/glyph-1024.png` there. Skip otherwise.

---

## 9. Icon audit script (PowerShell, run at the end)

```powershell
Add-Type -AssemblyName System.Drawing
Get-ChildItem src-tauri/icons -Filter *.png | ForEach-Object {
  $img = [System.Drawing.Image]::FromFile($_.FullName)
  "{0,-28} {1}x{2}" -f $_.Name, $img.Width, $img.Height
  $img.Dispose()
}
ico = [System.Drawing.Icon]::ExtractAssociatedIcon((Resolve-Path src-tauri/icons/icon.ico).Path)
"icon.ico OK: $((ico.Width))x$((ico.Height))"
```

All PNGs must report exactly the dimension their filename promises; `icon.ico` must report 256 after extraction (largest frame).

---

## 10. Verification plan

1. **Frontend gate:** `npm ci && npm run build` — must pass with the new SVG imports.
2. **Rust gates (only if you touched `src-tauri/` code — tray task):**
   - `cargo fmt --check --edition 2024`
   - `cargo clippy -- -D warnings` (zero warnings allowed)
   - `cargo test` (expect the existing suite green; `manager_test` L136/L699 are known flaky — rerun once before investigating)
3. **Icon integrity:** §9 audit script output clean.
4. **Manual matrix (after `npm run tauri dev` / `npm run tauri build`):**

   | # | Surface | Expected |
   |---|---|---|
   | 1 | Windows taskbar (running app) | cyan kingfisher on ink squircle |
   | 2 | Window title bar | same mark, small |
   | 3 | `rapids.exe` in Explorer | same mark |
   | 4 | NSIS installer + Start-menu shortcut | same mark |
   | 5 | App header/sidebar | glyph + `RAPIDS` wordmark, no placeholder SVGs left |
   | 6 | Browser tab (dev server opened in a browser) | new favicon |
   | 7 | Tray (if tray exists) | crisp glyph at 16–32 px, legible on dark and light taskbars |
   | 8 | Extension toolbar (if extension exists, load unpacked) | legible glyph at 16 px, crisp at 48/128 |
5. **Regression:** download a file with the app (any direct URL) to confirm the branding task broke nothing.

---

## 11. Acceptance checklist (definition of done)

- [ ] `src-tauri/icons/` fully regenerated from `brand/master/app-icon.png` (incl. `icon.icns`) and `bundle.icon` paths valid.
- [ ] `brand/` committed with masters, palette tokens, tray/extension/web assets, KIT-SHEET.
- [ ] `index.html` favicon + title updated; no `tauri.svg`/`react.svg`/`vite.svg` placeholders referenced anywhere.
- [ ] Header shows glyph + `RAPIDS` wordmark; UI accent colors untouched (still green — intentional).
- [ ] Extension icons wired if extension exists, otherwise staged + documented.
- [ ] Tray updated if it existed, otherwise staged + documented.
- [ ] README header added.
- [ ] All quality gates pass; manual matrix items 1–6 verified (7–8 per conditionality).
- [ ] No changes outside: `brand/`, `src-tauri/icons/`, `src-tauri/tauri.conf.json` (icon paths only), `index.html`, `public/`, `src/assets/`, the header/about components touched for branding, `extension/` icons+manifest, `README.md`.

## 12. Report format

Return a concise report:
1. Branch + commit list.
2. Files added/changed (grouped per task).
3. Output of the §9 icon audit.
4. Manual matrix results (screenshots welcome for items 1, 3, 5).
5. Anything skipped *because the target didn't exist* (tray / extension / about), so the follow-up tasks can consume `brand/` later.

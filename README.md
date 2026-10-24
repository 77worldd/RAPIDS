<p align="center"><img src="brand/tauri-icons/128x128.png" width="96" alt="RAPIDS logo" /></p>
<h1 align="center">RAPIDS</h1>

RAPIDS is a Windows desktop HTTP(S) download manager with a persistent queue,
timed start windows, and per-task and app-wide transfer limits.

## Startup and resume

The SQLite setting `resume_on_start` is seeded to `true`. When the host starts
with that setting enabled, it changes persisted `downloading` tasks back to
`queued` and then processes the queue. With the setting disabled, this startup
requeue does not happen. Requeuing work does **not** by itself approve reuse of
any bytes from an interrupted download.

Only the engine's sidecar check decides whether saved bytes are resumable. It
requires a recorded range-capable transfer, a stored total size matching the
fresh server probe, a matching stored ETag (or, if no stored ETag, a matching
stored Last-Modified value), **and** a `data.part` file whose length matches
the fresh size. If any proof is absent or differs, the engine starts a fresh
transfer rather than treating persisted task progress as downloaded bytes.

## Browser extension

`extension/` is a zero-build Manifest V3 extension for Chrome and Edge. It
watches the active tab for direct media and large-file responses, scans page
links on demand, and hands the chosen link to RAPIDS through the registered
`rapids://` protocol. The extension does not bundle or import any npm package.

### Install

1. Open `chrome://extensions` (or `edge://extensions`) and enable **Developer
   mode**.
2. Choose **Load unpacked** and select the repository's `extension/` folder.
3. Pin the RAPIDS toolbar action.

The desktop app must have been started at least once so Windows knows the
`rapids` protocol. Debug builds register it during `npm run tauri dev`; packaged
builds register it from the installer.

### Use

- Open the toolbar popup to see direct download responses sniffed from the
  current tab. Choose **Scan page** to add links and media elements found in
  the page DOM. Each result has its own **→ RAPIDS** action.
- Right-click a direct file link and choose **Download with RAPIDS**. This works
  without opening the popup and also appears for links the network sniffer has
  already seen.
- The toolbar badge shows the number of candidates on the active tab. It is
  emerald while the tab has candidates and neutral when it does not.
- **Options** controls the network-response size threshold (1–1024 MB, default
  1 MB), optional automatic page scanning, and a test handoff to RAPIDS. The
  test appends a unique query marker so repeated tests are not mistaken for
  duplicates.
- Browser handoffs open the normal **Add download** sheet with a **Sent from
  browser** banner, the suggested name and size, and the source page. RAPIDS
  still derives the final file name from the URL and server response; `n` and
  `sz` are suggestions, not an override.
- **Settings → General → Ask before adding links sent from the browser** is on
  by default. Turning it off queues a browser handoff immediately instead of
  opening the confirmation sheet.

### Protocol

The extension builds a single deep link:

```text
rapids://add?u=<encoded-http(s)-url>&n=<name>&t=video|audio|file&p=<page>&sz=<bytes>&v=1
```

- `u` is required and must be an absolute HTTP or HTTPS URL.
- `n`, `t`, `p`, `sz`, and `v` are optional hints. Hints are validated when
  present; malformed values, an invalid `u`, an unsupported action, or a link
  longer than 4096 characters are rejected with a notice.
- Only the `add` action is accepted. Duplicate submissions of the same URL
  within 30 seconds are ignored.
- Warm and cold launches both use the validated pending-link inbox. A cold
  launch drains the inbox after the window is ready; a warm launch is forwarded
  to the existing instance and signals the frontend to drain again.

All protocol construction and validation is isolated in `extension/handoff.js`.
Replacing the deep link with Native Messaging later should not require changes
to the sniffer, popup, or options UI.

### Permissions and storage

The extension requests `contextMenus`, `storage`, `notifications`, `tabs`,
`webRequest`, and host access to all sites. Host access is needed to observe
response headers and to offer download candidates on arbitrary pages; request
bodies and page content are not uploaded anywhere.

Candidate storage is tab-scoped, expires after 30 minutes, and is capped at 200
entries per tab and 2000 entries overall. Closing a tab drops its bucket.

### Development checks

```powershell
npm run test:extension
npm run test:deeplink
npm run lint
npm run typecheck
```

The manual browser matrix uses Chrome or Edge with the unpacked extension. On
Windows, the native **Open RAPIDS?** prompt is part of the OS protocol handoff
and must be accepted once per origin unless **Always allow** is checked.

### Limitations

- The extension targets Chromium (Chrome and Edge). Firefox and Safari are not
  supported by this MV3 build.
- DRM-protected, `blob:`, and Media Source Extension streams are not direct
  HTTP downloads and are not offered.
- Browser-internal pages and extension stores do not run content scripts, so
  page scanning is unavailable there; link context menus and the popup's
  network results still work where the browser permits them.
- The fallback path opens a short-lived tab for the `rapids://` link when an
  in-page anchor cannot be used. The tab closes after 20 seconds or as soon as
  the browser hands the link to the OS.
- Signing, store packaging, and automatic extension updates are outside this
  repository's build.

See [ARCHITECTURE.md](ARCHITECTURE.md) for the implemented modules, storage
formats, and M2 verification record.

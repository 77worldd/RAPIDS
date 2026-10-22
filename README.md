# RAPIDS

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

See [ARCHITECTURE.md](ARCHITECTURE.md) for the implemented modules, storage
formats, and M2 verification record.

# smix-server

smix's general-purpose httpapi server. Fixed httpapi stack — `axum` / `sqlx` (postgres) / `redis` (valkey). The first feature module, `mod stream`, is the observability surface for sim test live-view: a REST sim registry plus a `tower-http` ServeDir mount that serves per-sim live HLS (`index.m3u8 + seg_*.ts`). The sibling `mod capture` actively orchestrates the recording pipeline that produces that HLS (rolling `simctl recordVideo` → a single continuous ffmpeg encoder fed via a raw-video FIFO → live mpegts HLS, 0 `EXT-X-DISCONTINUITY`), exposed as `POST /api/capture/start` / `POST /api/capture/stop`; `GET /api/sims` reflects live capture state from valkey. Future capabilities (metrics, control API) attach to the same server as sibling modules.

Part of the [smix](https://github.com/goliajp/smix) workspace. See the
top-level [README.md](../../README.md) for the project overview.

## Configuration

Reads from the environment (via `dotenvy`, `.env.local` honored):

- `SMIX_SERVER_BIND` — listen address (default `127.0.0.1:8787`)
- `DATABASE_URL` — postgres connection string (required)
- `REDIS_URL` — valkey connection string (required)
- `SMIX_STREAM_ROOT` — HLS root dir served under `/streams` (default `.smix/stream`)

## License

Dual-licensed under either Apache License 2.0 or MIT, at your option.

-- One row per sim whose live HLS stream is being served. PK = udid;
-- stream_path is the HLS dir relative to SMIX_STREAM_ROOT (served under
-- /streams/<stream_path>/index.m3u8). Segment index / access audit /
-- replay archive tables are deliberately NOT pre-created (cold plan:
-- "c2 MVP 接通连接 + 最小 schema, 不堆未用表").
CREATE TABLE IF NOT EXISTS stream_sessions (
    udid        TEXT PRIMARY KEY,
    device_name TEXT NOT NULL DEFAULT '',
    runtime     TEXT NOT NULL DEFAULT '',
    stream_path TEXT NOT NULL,
    started_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

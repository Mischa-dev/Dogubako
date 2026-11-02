---
title: Operations
description: Back up, upgrade, monitor and size a Dogubako server.
sidebar:
  order: 7
---

## What's in the data directory

| Path | Holds | Back up? |
|---|---|---|
| `dogubako.db` | SQLite database: accounts, sessions, tokens, policies, job history, audit log, file index | **Yes** |
| `dogubako.toml` | Optional config file | **Yes** |
| `extensions/` | Extension manifests and `trusted.toml` | **Yes** |
| `models/` | Downloaded models | Optional; they can be downloaded again |
| `files/` | Uploads and outputs, which expire after the retention period | No |

## Backups

The safest backup is taken while the server keeps running, with SQLite's
online backup:

```sh
sqlite3 data/dogubako.db ".backup 'backup/dogubako.db'"
cp data/dogubako.toml backup/ 2>/dev/null || true
cp -r data/extensions backup/
```

Or stop the server and copy the files. To restore, stop the server, put the
files back in the data directory, and start it again.

## Upgrading

1. Back up `dogubako.db`.
2. Update:
   - **Docker:** `git pull && docker compose up -d --build`
   - **Native:** build or download the new binary, replace
     `/usr/local/bin/dogubako`, then `sudo systemctl restart dogubako`.
3. Check that it's healthy: `dogubako server health` (or `docker compose ps`).

The database schema is updated automatically on start.

## Health and logs

- `GET /healthz` returns `200` when the server is up. `dogubako server health`
  does the same check for container health checks; the Docker image runs it
  every 30 seconds.
- `GET /api/v1/status` is public and reports the version, auth mode and
  whether setup is still required.
- **Admin → Instance** (or `GET /api/v1/admin/config`) shows the effective
  configuration, running jobs and uptime.
- Logs go to stderr: `docker compose logs -f dogubako` or
  `journalctl -u dogubako -f`. Raise the detail with `DOGUBAKO_LOG=debug`.
  Command arguments are never logged, because some carry secrets such as
  archive passwords.

## Retention and cleanup

- Uploads and outputs are deleted `file_retention_hours` after they are
  created (24 hours by default).
- Job records are deleted after `job_retention_days` (14 by default).
- Each account has a storage quota (`user_quota_mb`, 10 GB by default). An
  upload that would go over it is refused with "storage quota exceeded".
- Cleanup runs periodically in the background; nothing needs scheduling.

## Sizing

- **CPU.** Video transcoding and speech recognition are the heavy jobs, and
  FFmpeg and whisper.cpp each use several threads. Set
  `max_concurrent_jobs` to about your core count divided by two for media
  work, or leave the default (the core count) for mixed use.
- **Memory.** Keep the container or service memory limit above
  `exec_memory_mb × max_concurrent_jobs`. Idle, the server uses about 30 MB.
- **Disk.** Leave room for `user_quota_mb × accounts` in `data/files`, plus
  models (Whisper models are 75 MB to 1.5 GB).

## Security checklist

- [ ] Reachable only where you need it: loopback, tailnet or behind HTTPS.
- [ ] `secure_cookies = true` when served over HTTPS.
- [ ] Agents have only the tools and permissions they need, and compact
      discovery if there are many of them.
- [ ] `fs_roots` is empty unless you need folder tools, and mounts are
      read-only.
- [ ] `allow_private_network` stays `false`.
- [ ] On native Linux installs, `sandbox = "bwrap"`.
- [ ] Old tokens are revoked; the audit log is reviewed now and then.

See [Security](/project/security/) for the full threat model.

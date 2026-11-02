---
title: Configuration
description: How a Dogubako server is configured, and the settings worth changing for common setups.
sidebar:
  order: 6
---

## Where settings come from

From lowest to highest priority:

1. Built-in defaults.
2. A TOML file: `<data_dir>/dogubako.toml`, or the file given with `--config`
   or `DOGUBAKO_CONFIG`. Start from
   [`deploy/dogubako.example.toml`](https://github.com/mischa-dev/dogubako/blob/main/deploy/dogubako.example.toml).
3. Environment variables: `DOGUBAKO_` plus the key in upper case, for example
   `max_upload_mb` is `DOGUBAKO_MAX_UPLOAD_MB`.
4. Command-line flags: `--bind`, `--data-dir`, `--config`, `--public-url`,
   `--no-auth`.

`dogubako server config` prints the effective configuration as TOML, which is
the quickest way to check what the server will use.

```toml
# <data_dir>/dogubako.toml
bind = "0.0.0.0:8420"
public_url = "https://tools.example.com"
instance_name = "Home toolbox"
secure_cookies = true
max_upload_mb = 4096
file_retention_hours = 48
sandbox = "bwrap"
```

Every setting is listed in the [configuration reference](/reference/configuration/).

## Common setups

### A household server

A few trusted people, files kept for a couple of days:

```sh
DOGUBAKO_INSTANCE_NAME="Home toolbox"
DOGUBAKO_FILE_RETENTION_HOURS=72
DOGUBAKO_USER_QUOTA_MB=20480
```

### A shared server for AI agents

Many agents, mostly short jobs, tight isolation:

```sh
DOGUBAKO_MCP_DISCOVERY=compact          # agents search for tools instead of listing all 249
DOGUBAKO_PER_USER_CONCURRENCY=1
DOGUBAKO_USER_QUOTA_MB=2048
DOGUBAKO_FILE_RETENTION_HOURS=6
DOGUBAKO_SANDBOX=bwrap                  # native Linux installs
```

### A video and transcription box

Few users, long CPU-heavy jobs:

```sh
DOGUBAKO_MAX_CONCURRENT_JOBS=2          # FFmpeg and whisper.cpp use several threads each
DOGUBAKO_MAX_JOB_SECONDS=14400          # four hours
DOGUBAKO_EXEC_CPU_SECS=14400
DOGUBAKO_EXEC_MEMORY_MB=8192
DOGUBAKO_MAX_UPLOAD_MB=8192
```

Keep the container or service memory limit above
`EXEC_MEMORY_MB × MAX_CONCURRENT_JOBS`.

### Folder tools on a NAS

Let tools such as duplicate finders and folder size read your shares:

```sh
DOGUBAKO_FS_ROOTS=/srv/photos,/srv/documents
```

Then grant `host_fs_read` to the accounts that should use them. Paths are
resolved (symlinks included) and must stay inside a root.

## Engine paths

Point the server at a specific engine binary with `DOGUBAKO_<ID>_PATH`, for
example `DOGUBAKO_FFMPEG_PATH=/opt/ffmpeg/bin/ffmpeg`. The ids are listed in
[Engines and models](/app/engines-and-models/#engines).

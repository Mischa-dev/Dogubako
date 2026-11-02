---
title: Configuration reference
description: Every Dogubako server setting, with its TOML key, environment variable and default.
---

Set each value as a TOML key in `<data_dir>/dogubako.toml`, or as the
environment variable shown. See [Configuration](/self-hosting/configuration/)
for precedence and examples.

## Server settings

| TOML key | Environment variable | Default | Meaning |
|---|---|---|---|
| `bind` | `DOGUBAKO_BIND` | `127.0.0.1:8420` | Listen address. The Docker image uses `0.0.0.0:8420` inside the container |
| `data_dir` | `DOGUBAKO_DATA_DIR` | `./data` | Database, files, models, extensions and the config file |
| `public_url` | `DOGUBAKO_PUBLIC_URL` | — | External base URL, used for absolute links given to agents |
| `instance_name` | `DOGUBAKO_INSTANCE_NAME` | `Dogubako` | Shown in the UI and to MCP clients |
| `auth` | `DOGUBAKO_AUTH` | `required` | `none` is allowed only on a loopback address |
| `allow_insecure_no_auth` | `DOGUBAKO_ALLOW_INSECURE_NO_AUTH` | `false` | Allow `auth = "none"` on a non-loopback address. Almost never what you want |
| — | `DOGUBAKO_SETUP_TOKEN` | random | A fixed first-run setup token instead of a random one |
| `secure_cookies` | `DOGUBAKO_SECURE_COOKIES` | `false` | Mark cookies `Secure`. Turn on whenever the server is reached over HTTPS |
| `session_hours` | `DOGUBAKO_SESSION_HOURS` | `336` | Browser session lifetime (14 days) |
| `max_upload_mb` | `DOGUBAKO_MAX_UPLOAD_MB` | `2048` | Largest upload |
| `user_quota_mb` | `DOGUBAKO_USER_QUOTA_MB` | `10240` | Default storage quota per account |
| `file_retention_hours` | `DOGUBAKO_FILE_RETENTION_HOURS` | `24` | Uploads and outputs are deleted after this long |
| `job_retention_days` | `DOGUBAKO_JOB_RETENTION_DAYS` | `14` | Job history is kept this long |
| `max_concurrent_jobs` | `DOGUBAKO_MAX_CONCURRENT_JOBS` | CPU count, at most 16 | Jobs running at once across the server |
| `per_user_concurrency` | `DOGUBAKO_PER_USER_CONCURRENCY` | `2` | Default jobs at once per account |
| `max_job_seconds` | `DOGUBAKO_MAX_JOB_SECONDS` | `3600` | Hard wall-clock limit per job |
| `exec_memory_mb` | `DOGUBAKO_EXEC_MEMORY_MB` | `4096` | Address-space limit per external program (`0` turns it off). Unix only |
| `exec_cpu_secs` | `DOGUBAKO_EXEC_CPU_SECS` | `3600` | CPU-time limit per external program. Unix only |
| `sandbox` | `DOGUBAKO_SANDBOX` | `none` | `bwrap` sandboxes external programs with bubblewrap (Linux) |
| `fs_roots` | `DOGUBAKO_FS_ROOTS` | `[]` | Host folders that folder tools may use. Comma-separated in the environment |
| `allow_private_network` | `DOGUBAKO_ALLOW_PRIVATE_NETWORK` | `false` | Let network tools reach private, loopback and link-local addresses |
| `mcp_enabled` | `DOGUBAKO_MCP_ENABLED` | `true` | Serve MCP at `/mcp` |
| `mcp_discovery` | `DOGUBAKO_MCP_DISCOVERY` | `full` | `full` or `compact`; see [Discovery](/agents/discovery-and-results/) |
| `allowed_origins` | `DOGUBAKO_ALLOWED_ORIGINS` | `[]` | Extra `Origin`s allowed to call `/mcp` (browser-based clients). Comma-separated |
| `models_dir` | `DOGUBAKO_MODELS_DIR` | `<data_dir>/models` | Model storage |
| `extensions_dir` | `DOGUBAKO_EXTENSIONS_DIR` | `<data_dir>/extensions` | Trusted TOML extensions |

## Plain words

Settings for the [plain words](/app/plain-words/) relay at
`/api/v1/assistant/*`.

| TOML key | Environment variable | Default | Meaning |
|---|---|---|---|
| `assistant_enabled` | `DOGUBAKO_ASSISTANT_ENABLED` | `true` | Allow plain words on this server. People still have to turn it on and pick a model |
| `assistant_allow_private` | `DOGUBAKO_ASSISTANT_ALLOW_PRIVATE` | `false` | Let the relay reach loopback and LAN model servers, such as Ollama on this host |
| `assistant_base_url` | `DOGUBAKO_ASSISTANT_BASE_URL` | — | A shared OpenAI-compatible endpoint, offered to everyone as "This server" |
| `assistant_api_key` | `DOGUBAKO_ASSISTANT_API_KEY` | — | Its API key. Never sent to browsers |
| `assistant_model` | `DOGUBAKO_ASSISTANT_MODEL` | — | Its model id. When set, every request through "This server" uses it |

## Environment-only settings

These apply to the server, the CLI, the MCP server and the desktop app.

| Variable | Meaning |
|---|---|
| `DOGUBAKO_CONFIG` | Path to the TOML config file (same as `--config`) |
| `DOGUBAKO_HOME` | Base folder for models, extensions and `bin` for the CLI, stdio MCP server and desktop app |
| `DOGUBAKO_<ID>_PATH` | Explicit path to an engine, such as `DOGUBAKO_FFMPEG_PATH` or `DOGUBAKO_7ZIP_PATH` |
| `DOGUBAKO_ENGINES` | How the macOS app uses its managed engines: unset (only its own copies of the engines it manages), `prefer` (its copies first, then the system) or `system` (ignore them). The Docker image sets `system`. Without a managed set (CLI, native server, development builds) it changes nothing |
| `DOGUBAKO_LOG` | Log filter, such as `info`, `debug` or `dogubako_server=debug` |

## Command-line flags

`dogubako server start` accepts `--bind`, `--data-dir`, `--config`,
`--public-url` and `--no-auth`, which override the file and the environment.
`--no-auth` only works with a loopback `--bind`.

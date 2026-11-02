---
title: Key concepts
description: Tools, specs, engines, models, jobs, artifacts, permissions and policies, the words the rest of these docs use.
sidebar:
  order: 4
---

A few ideas explain how every part of Dogubako behaves.

## Tools and specs

A **tool** does one job, such as `image.convert` or `pdf.merge`. Its id is
`<category>.<name>` and never changes, so scripts and agent policies can rely
on it.

Each tool is described by a **spec**: its name, description, keywords,
parameters (with types, defaults, ranges and accepted file types), the kind of
output it produces, the engines and permissions it needs, and the platforms
it supports. The app's forms, the CLI flags, the HTTP API schema and the MCP
tool definitions are all generated from that one spec.

| Interface | How `image.convert` appears |
|---|---|
| App | **Images → Convert Images**, with a generated form |
| CLI | `dogubako image convert photo.heic --format webp` |
| HTTP | `POST /api/v1/tools/image.convert/run` |
| MCP | the tool `image_convert` (dots become underscores) |

## Categories

Tools are grouped into twelve categories: `image`, `video`, `audio`, `speech`,
`pdf`, `files`, `text`, `dev`, `data`, `network`, `design` and `system`. The
category id is the first part of every tool id and the first word of every
CLI tool command.

## Engines

Some tools run entirely inside Dogubako (**native**). Others call a mature
external program (**external**), such as FFmpeg for video or Tesseract for
OCR. Dogubako runs them as separate programs and never links them. The macOS
app brings its own pinned copies (some built in, the rest downloaded the first
time you need them), the Docker image has all of them baked in, and the CLI
finds them on your `PATH`, in its own `bin` folder, or at a path you set with
`DOGUBAKO_<ID>_PATH`. When one is missing, the tools that need it stay listed,
are marked unavailable, and explain how to get it. See
[Engines and models](/app/engines-and-models/).

## Models

A few tools use small machine-learning models that run locally: whisper.cpp
models for transcription, a voice-activity detector, a face detector, and
Piper voices for text to speech. Models are never bundled. You download the
ones you want, and Dogubako verifies them by checksum where upstream
publishes one. See [Engines and models](/app/engines-and-models/).

## Runs, jobs and artifacts

Running a tool is a **run**. On a server each run is a **job** with an id
(`j_…`), a status (`queued`, `running`, `succeeded`, `failed` or
`cancelled`), progress, and a result.

A result can contain:

- **text**, with a MIME type such as `text/plain`, `application/json` or
  `text/markdown`;
- structured **data**, which the app renders as tables, color swatches,
  diffs, highlighted regex matches or boxes over an image;
- **artifacts**, the output files, each with a name, MIME type and size;
- **warnings**, for things that worked but deserve a look.

Inputs and outputs on a server are **files** addressed by random ids (`f_…`).
They belong to whoever uploaded or produced them and expire after the
retention period (24 hours by default).

## Permissions

Most tools only read the inputs you give them. Tools that do more declare a
**permission**:

| Permission | Lets a tool… |
|---|---|
| `network` | Make outbound network requests (DNS, HTTP, TLS checks…) |
| `host_fs_read` | Read folders on the host, inside configured roots |
| `host_fs_write` | Change files on the host, inside configured roots |
| `system_info` | Read hardware and OS information |
| `process_info` | List running processes |

On the desktop and the CLI you are trusted like your own shell. On a server,
each account and token is granted permissions explicitly.

## Accounts, agents and policies

A self-hosted server has **accounts** with a role: `owner`, `admin`, `user`
or `agent`. Each account has a **policy** that says which tools it may run
(`allow` and `deny` patterns such as `image.*`), which permissions it has, and
its concurrency and storage limits. **API tokens** act as their account and
can be narrowed further with a scope. Agents start with nothing and are
granted exactly what they need. See
[Permissions and policies](/reference/permissions-and-policies/).

## Extensions

You can add tools without writing Rust by describing a command-line program
in a small TOML **extension** manifest. An extension loads only after you
explicitly trust it, and it runs through the same engine, limits and policies
as built-in tools. See [Extensions](/develop/extensions/).

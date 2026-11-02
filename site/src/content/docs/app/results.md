---
title: Results and history
description: How Dogubako shows results, how to save outputs, and where past runs and files live.
sidebar:
  order: 5
---

A run can produce text, structured data, output files, or all three. The app
picks the right view for each automatically.

## Output files

Each output file is listed with its name, type and size, and can be previewed
inline:

- **Images** show a **Before / after** slider against the original, or just
  the result.
- **Audio and video** play in place. **PDFs** open in a viewer.
- **Text, JSON, CSV and subtitles** show as text.

To keep a file:

- In a browser, click **Download**, or **Download all (.zip)** when there are
  several.
- In the desktop app, click **Save…** to choose where it goes, or **Show in
  folder** to reveal it in your file manager. Outputs are written to your
  [outputs folder](/app/desktop/#where-files-go) first.

## Text and data

| The tool returns | You see |
|---|---|
| Plain text | A text block with a copy button |
| JSON | A collapsible **Tree** or the **Source** |
| Markdown, HTML, SVG | A rendered **Preview** (HTML in a sandboxed frame) or the **Source** |
| A table | A table you can sort by any column and copy as CSV |
| Colors | Swatches with their hex values, each one copyable |
| A diff | Insertions and deletions highlighted inline |
| Regions in an image | Boxes drawn over the image, with labels and scores |
| Regex matches | The matches highlighted in your text, with their groups |
| Anything else | A key and value list, with a tree for nested values |

## Warnings and errors

**Warnings** appear above the result when the tool finished but something
deserves a look, for example a value that had to be adjusted.

When a run fails, the error says what went wrong and, where it can, how to
fix it: the parameter that was invalid, the engine that is missing with its
install command, or the external program's own error output. See
[Errors](/reference/errors/) for every error code.

## History

**History** lists every run with its tool, status, how long ago it ran and
how long it took. Filter by tool or status, and:

- **Open with same settings** reopens the tool with that run's parameters.
- **Delete run** removes it. On a server this also deletes its output files.

In the desktop app, history is stored on this device. Turn it off in
**Settings → Keep run history**. On a server, history belongs to your account
and job records are kept for 14 days by default; live runs are not recorded
in history.

## Files on a server

When you use a self-hosted server, **Files** lists your uploads and outputs
with your storage use and quota. Files expire after the retention period
(24 hours by default) and can be deleted sooner. Other accounts can never see
your files.

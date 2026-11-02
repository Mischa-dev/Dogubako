---
title: Recipes
description: Copy-and-paste dogubako commands for everyday jobs with images, video, audio, PDFs, files, text and data.
sidebar:
  order: 2
---

Short, real commands for common jobs. Every tool has more options than shown
here: run `dogubako <category> <tool> --help`, or look the tool up in the
[catalog](/tools/).

## Images

```sh
# Convert iPhone photos to WebP (HEIC needs ImageMagick)
dogubako image convert *.heic --format webp --quality 80 -o webp/

# Shrink a folder of photos for the web: longest edge 1600 px, re-encoded
dogubako image compress photos/*.jpg --max-dimension 1600 -o web/

# Remove GPS and camera metadata before sharing
dogubako image strip-metadata holiday.jpg

# Blur every face in a photo (needs the seeta-face model)
dogubako models download seeta-face
dogubako image face-blur group.jpg

# Read the text in a screenshot (needs Tesseract)
dogubako image ocr screenshot.png

# Make a full favicon pack from a logo
dogubako image favicon logo.png -o favicon/
```

## Video

```sh
# Compress a screen recording to a sensible size (needs FFmpeg)
dogubako video compress talk.mov

# Cut the first 30 seconds into a new clip
dogubako video trim talk.mov --start 0 --end 30

# Turn a short clip into a GIF
dogubako video to-gif clip.mp4

# Pull the audio track out of a video
dogubako video extract-audio interview.mp4
```

## Audio and speech

```sh
# Normalize a podcast episode to broadcast loudness (EBU R128)
dogubako audio normalize episode.wav

# Remove long silences from a recording
dogubako audio remove-silence lecture.m4a

# Transcribe offline to subtitles and plain text (needs whisper.cpp and a model)
dogubako models download whisper-base
dogubako speech transcribe interview.m4a --formats srt,txt

# Shift subtitles that are two seconds late (negative numbers need the = form)
dogubako speech subtitle-shift --file movie.srt --offset=-2
```

## PDFs

```sh
# Merge (needs qpdf), then compress (needs qpdf and Ghostscript)
dogubako pdf merge chapter-*.pdf -o book.pdf
dogubako pdf compress book.pdf

# Make a scanned PDF searchable (needs Tesseract, Poppler and qpdf)
dogubako pdf ocr scan.pdf

# Pull out pages 3 to 5
dogubako pdf extract-pages report.pdf --pages 3-5

# Get the text out of a PDF
dogubako pdf extract-text contract.pdf         # prints the text and saves contract.txt
```

## Files

```sh
# Several checksums of a download at once
dogubako files hash big.iso --algorithms sha256,blake3

# Check a download against its published SHA256SUMS list
dogubako files checksum-verify ubuntu.iso --checksums "$(cat SHA256SUMS)"

# Look inside an archive without extracting it
dogubako files archive-list backup.zip

# Find duplicate files in a folder (reads host folders: the CLI trusts you like your shell)
dogubako files duplicates --path ~/Pictures
```

## Text and developer tools

```sh
# Pretty-print JSON from the clipboard or another command
pbpaste | dogubako dev json

# Convert YAML to JSON
dogubako dev convert --from yaml --to json < config.yaml

# Hashes of a string
dogubako dev hash hello

# Ten UUIDs
dogubako dev uuid --count 10

# Decode a JWT without trusting it
dogubako dev jwt "$TOKEN"

# Change case
dogubako text case "Hello World" --case snake
```

## Data

```sh
# Summary statistics for every column of a CSV
dogubako data stats sales.csv

# CSV to JSON
dogubako data csv-to-json sales.csv            # writes sales.json

# Clean up a messy export: trim cells, blank out placeholders like N/A, drop empty rows and columns
dogubako data clean export.csv
```

## Network

```sh
dogubako network dns-lookup example.com
dogubako network tls-inspect example.com
dogubako network http-headers https://example.com
dogubako network cidr 10.0.0.0/22
```

## System

```sh
dogubako system overview
dogubako system battery
```

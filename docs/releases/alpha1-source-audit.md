# Alpha 1 release verification, 2026-11-03

Release: https://github.com/Mischa-dev/Dogubako/releases/tag/v0.1.0-alpha.1

The tag still points to `7383432f0e486923dee424315c192fc315fdb4ac`.
Release run https://github.com/Mischa-dev/Dogubako/actions/runs/41540947261
completed successfully. All seven jobs succeeded, including the source bundle,
three CLI builds, container image and checksums. The early one-asset snapshot
was taken before uploads finished.

## Published files verified

All 14 assets were downloaded and their sizes and SHA-256 digests matched
GitHub's release metadata. `SHA256SUMS.txt` covers exactly the other 13 files,
with no missing or extra entries, and its recorded hashes match those bytes.

| Kind | Published files |
|---|---|
| macOS desktop | `Dogubako_0.1.0-alpha.1_aarch64.dmg`, `Dogubako_aarch64.app.tar.gz` |
| CLI/server | `dogubako-aarch64-apple-darwin`, `dogubako-x86_64-unknown-linux-gnu`, `dogubako-x86_64-pc-windows-msvc.exe` |
| Optional engines | `ghostscript-10.08.0-darwin-arm64.tar.gz`, `imagemagick-7.1.2-35-darwin-arm64.tar.gz`, `pandoc-3.13-darwin-arm64.tar.gz`, `realesrgan-0.2.5.0-darwin-arm64.tar.gz`, `tesseract-5.5.4-darwin-arm64.tar.gz`, `whisper-v1.7.6-darwin-arm64.tar.gz` |
| Index and sources | `packs.json`, `engine-sources-0.1.0-alpha.1.tar` |
| Checksums | `SHA256SUMS.txt` |

The app's embedded engine manifest and each optional pack's embedded manifest
identify the original binary lock, `c6a577794370`. That value matches the first
12 characters of the SHA-256 of the tagged `engines.lock.json`. All advertised
engine versions match that lock. The app's on-demand index equals `packs.json`,
and all six pack files match its hashes and download sizes.

The published source archive has SHA-256
`9e8e2b0c2b5adb4f7290ed4171f22d748db549c23c3e85cfa684b285d5ca7bda`.
Its 47 source members match the hashes in `SOURCES.md` and the source pins
introduced by PR #13. The x264 index records the pinned full commit. These
checks establish archive integrity, not that the original bundle contains
every build input.

## Remaining defects

PR #13 already removed the current-version URL substitution after the release.
Its binary bottle versions, URLs and hashes are identical to the tagged ones.
NSS's pinned URL correctly retains both `NSS_3_132_RTM` and `nss-3.132`.

Inspection of the exact `.brew/*.rb` recipes inside the 47 checksum-verified
bottles found two remaining problems:

1. `jbig2dec 0.20` was pinned to the upstream release distribution archive,
   while the shipped bottle recipe uses the GitHub tag archive. They share a
   version but have different archive hashes. The corrected source pin uses
   the bottle's tag archive, SHA-256
   `a9705369a6633aba532693450ec802c562397e1b824662de809ede92f67aff21`.
2. The bundle omits 19 supplemental build inputs: six Readline patches, two
   GnuPG patches, one GnuTLS patch, two Homebrew local patches, five ExifTool
   Perl resources, Ghostscript fonts, GLib's gobject-introspection source, and
   Poppler font data. Test-only resources are excluded. Recipe code also
   contains inline source edits needed to explain how the bottles were built.

The two Homebrew local patches are pinned to commit
`046433224d7dab924cb05164a4b78aa7292f6f89`. At that commit, both recipes
match the bottle copies byte-for-byte after removing only the `bottle do`
checksum block. Their patch bytes are SHA-256 pinned in `source-inputs.json`.

The new collector verifies every recipe against its bottle and reviewed recipe
hash, verifies the upstream hash or commit occurs in that recipe, downloads
all supplemental inputs by hash, and includes the recipes, source-input review
and full binary lock in the archive. A changed bottle or upstream pin requires
a fresh review. It writes a complete archive only after all downloads pass;
failed attempts cannot overwrite an earlier complete archive or include stale
staging files.

## Reproduce and maintain

```sh
python3 -I packaging/engines/lock.py --check
python3 -I packaging/release/engine_sources.py --version check --check
python3 -I -m unittest discover -s packaging/release -p 'test_*.py' -v
python3 -I packaging/release/engine_sources.py --version 0.1.0-alpha.1-complete
```

For each engine update, review the recipe from the new checksum-pinned bottle
and update `packaging/engines/source-inputs.json`. Record the recipe hash,
matching bottle hash and source pin, and every non-test resource and patch.
Use immutable historical formula revisions for Homebrew-local patch files.
Do not infer URLs by replacing version strings or carry a review across a
changed bottle. The Engines workflow runs both offline regression tests and a
full source-bundle download without publishing anything.

`--lock` and `--inputs` accept explicit frozen files. To prepare sources for an
older binary release, retain its exact bottle versions, URLs and hashes while
adding verified source metadata. Do not regenerate its binary pins from the
current Homebrew API. A source metadata edit changes the lock fingerprint, so
compare binary records as well as recording the original embedded lock ID.

## Public release repair proposal, awaiting approval

Preserve the published tag, publication status, all 14 existing asset IDs and
bytes, and the existing `SHA256SUMS.txt`. Do not rerun the tag workflow: its
upload steps use `--clobber` and can replace assets.

After approval, add two uniquely named files to the same release:

- `engine-sources-0.1.0-alpha.1-complete.tar`: corrected exact-source archive
  with all 47 recipes, 47 upstream sources, 19 supplemental inputs and pins.
- `SHA256SUMS-engine-sources-complete.txt`: SHA-256 of that new archive only.

Also append this paragraph to the existing release body, preserving its text:

> Source bundle correction: `engine-sources-0.1.0-alpha.1-complete.tar` adds the
> exact Homebrew build recipes, patches and resources, and the jbig2dec source
> archive used by the shipped bottle. Use this complete bundle for the alpha's
> engine sources. Verify it with `SHA256SUMS-engine-sources-complete.txt`.
> Existing downloads and `SHA256SUMS.txt` are preserved.

This audit does not run the macOS app on a clean Mac, compare rebuilt binaries
bit-for-bit, or assess the full source obligations of every system package in
the container image. Subsequent Docker and Podman changes on main are not part
of this published tag.

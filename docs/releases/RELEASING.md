# Releasing Dogubako

Everything up to shipping is automatic; shipping is one run by Mischa.
Nothing tags, publishes a release, pushes an image or deploys the site
before that run.

## 1. Prepare (a pull request)

```sh
python3 -I packaging/release/release.py bump 0.1.0-alpha.3
cargo build -p dogubako-cli && python3 -I packaging/release/release.py regen
python3 -I packaging/release/notices.py --fetch-sources
```

`bump` sets the version in both Cargo workspaces and lockfiles,
`tauri.conf.json` and the web UI's package files, turns `[Unreleased]` in
CHANGELOG.md into the new version's section, and scaffolds
`docs/releases/v<version>.md` (the GitHub release body). `regen` rebuilds
`docs/CAPABILITIES.md` and the docs site's tool pages.

Then write the release note (everything below its `---` line is published),
and update the pages that name the current release.
`python3 -I packaging/release/release.py check` lists everything still
behind: version fields, the changelog entry, comments left in the note,
generated docs, and every line of README.md, website/ and the docs
install pages that still names an older release. Open the PR when it passes.

## 2. Draft (automatic)

When the bump merges, the **Release** workflow
([release.yml](../../.github/workflows/release.yml)) runs on main:

- `release.py check`, generated docs and third-party notices are current,
  the docs site builds, and the website's download links resolve to this
  release's assets.
- Builds the macOS app with its engines and engine packs, the Linux, macOS
  and Windows CLIs, the engine sources and the homelab image, and
  smoke-tests each.
- Leaves them in a **draft** release with the notes and `SHA256SUMS.txt`.
  A draft creates no tag and is not public. The image stays a workflow
  artifact; nothing is pushed to ghcr.io.

Fixes merged before shipping: run **Release** again by hand (Actions ›
Release › Run workflow on main). It replaces the draft for that version.

## 3. Ship (Mischa)

Check the draft on the
[releases page](https://github.com/Mischa-dev/Dogubako/releases): download
and open the DMG, read the notes. Then Actions › **Ship release** › Run
workflow with the version. It:

1. checks the draft is complete, built from that version, and matches
   `SHA256SUMS.txt`;
2. pushes the tested image as `ghcr.io/mischa-dev/dogubako:<version>`;
3. publishes the release, which creates tag `v<version>` at the draft's
   commit;
4. deploys dogubako.app with downloads pointing at the release, and
   docs.dogubako.app.

The docs site deploys on every push to main (docs.yml), but only while
main's version is a published release. From the bump merging until Ship
release, docs.dogubako.app keeps the previous release's pages; docs fixes
merged in that window go live with the release.

To make the run also wait for an approval click, add Mischa as a required
reviewer on the `release` environment (Settings › Environments).

## Rollback

A published tag is never moved. Mark the release as a pre-release or
delete it, re-run **Website** by hand with the previous release tag, and
ship a fix as the next version.

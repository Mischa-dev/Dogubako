# Launching 0.1.0-alpha.1

Everything for the first alpha is prepared on branches. What's left is
Mischa's: the decisions and accounts in part 1, then the go in part 3.
Nothing here has been merged, tagged, published or deployed.

## 1. Only Mischa can do these

- [ ] **Try the app.** Download the unsigned test DMG from the latest
      "Alpha test builds" run on PR #7 (artifact, every engine inside, no
      downloads needed). Open it with System Settings → Privacy & Security →
      Open Anyway. Things to try are in `docs/ALPHA_TESTING.md` (PR #7).
- [ ] **Listen to the trailer music.** `trailer/dist/dogubako-alpha-trailer.mp4`
      (PR #5, also in the project files). Nobody has heard it yet.
- [ ] **Trailer end card.** It says "Alpha · out now", which is true only once
      the release is public. `website/scripts/trailer.sh` (PR #8) refuses any
      trailer whose manifest isn't `in-development`, so pick one:
      re-render with "Alpha · coming soon" for the pre-launch site, or allow a
      `released` status on launch day.
- [x] **Pick the website concept.** Picked on 2026-11-02: Live product,
      reworked, with extra pages for self-hosting and agents. The website
      thread is rebuilding it on PR #3. Deploy it with the Website workflow's
      default `concept` (`current`). The other concepts stay under
      `website/concepts/` for later, and the workflow never publishes them.
- [ ] **Review the rebuilt website** on PR #3 before it merges.
- [ ] **Signing: decide signed or unsigned.**
      - Signed (recommended before sharing widely): enrol in the Apple
        Developer Program (USD 99/year, can take a day or two), create a
        **Developer ID Application** certificate, export it as a `.p12`, make
        an app-specific password at account.apple.com, then add the six
        repository secrets below.
      - Unsigned: ship as is. Testers open it once via Privacy & Security.
        Delete variant A in the release notes.
- [ ] **Cloudflare and DNS.** Create two Cloudflare Pages projects,
      `dogubako-website` and `dogubako-docs`. Add custom domains `dogubako.app`
      and `docs.dogubako.app` (Cloudflare creates the DNS records if the zone
      is on Cloudflare; otherwise add the CNAMEs it shows). Create an API token
      with *Cloudflare Pages: Edit* and add both Cloudflare secrets.

### Repository secrets (Settings → Secrets and variables → Actions)

| Secret | What | Needed for |
|---|---|---|
| `APPLE_CERTIFICATE` | base64 of the Developer ID `.p12` (`base64 -i cert.p12 \| pbcopy`) | signing |
| `APPLE_CERTIFICATE_PASSWORD` | the `.p12` export password | signing |
| `APPLE_SIGNING_IDENTITY` | `Developer ID Application: Your Name (TEAMID)` | signing |
| `APPLE_ID` | Apple account email | notarization |
| `APPLE_PASSWORD` | app-specific password | notarization |
| `APPLE_TEAM_ID` | 10-character team id | notarization |
| `CLOUDFLARE_API_TOKEN` | Pages: Edit token | website + docs deploy |
| `CLOUDFLARE_ACCOUNT_ID` | Cloudflare account id | website + docs deploy |

Without the Apple secrets the release builds an unsigned DMG; nothing fails.

## 2. Merge plan

All eight PRs plus this one merge cleanly when combined. The combined tree
builds, and the web tests (89) and every Rust workspace test pass. The one
conflict (#6 and #7 both adding desktop commands) is already resolved on #7.

The stack today:

```
main
 └─ #1 backend            claude/dogubako-toolbox
     ├─ #4 docs site      claude/project-thread-t69icb
     └─ #2 frontend       claude/brave-wright-yuzn0d
         ├─ #6 plain words  claude/project-thread-l241kf
         │   └─ #7 engines + release builds  claude/project-thread-j1bqd1
         ├─ #5 trailer      claude/project-thread-1zypgm
         ├─ #3 website      claude/project-thread-i6syyt
         │   └─ #8 website accuracy (Mischa)  review/prerelease-materials
         └─ #9 launch prep (this)  claude/project-thread-9dwqvf
```

Rules: use **Create a merge commit**, not squash or rebase. The branches share
history, and squashing a lower PR makes every PR above it conflict. After each
merge, change the next PR's base to `main` (Edit next to the title) before
merging it.

| Step | PR | Before merging |
|---|---|---|
| 1 | #1 backend → main | CI green (it is at f9f8511) |
| 2 | #2 frontend → main | Mischa has tried the app |
| 3 | #4 docs → main | Run `npm run gen:tools` in `site/` once more (backend changed after the docs were generated). Merging deploys the docs if the Cloudflare secrets exist. |
| 4 | #6 plain words → main | |
| 5 | #7 engines → main | Latest "Alpha test builds" run green |
| 6 | #9 launch prep → main | Regenerate notices (part 3) |
| 7 | #5 trailer → main | Music approved, end card decided |
| 8 | #8 → #3, then #3 → main | Rebuilt site reviewed |

Steps 1 to 6 are what the release needs. The website and trailer (7, 8) can
land later, because the release doesn't depend on them. `release.yml` (#7)
calls `packaging/macos/sign-engines.sh` and `packaging/release/engine_sources.py`
from this PR, so #7 and #9 must both be on `main` before tagging.

## 3. The go (in order)

1. Merge steps 1 to 6 above.
2. On `main`, regenerate and commit the notices:
   ```sh
   (cd web && npm ci)
   python3 -I packaging/release/notices.py --fetch-sources
   git commit -am "Regenerate third-party notices" && git push
   ```
3. Tag: `git tag -a v0.1.0-alpha.1 -m "Dogubako 0.1.0-alpha.1" && git push origin v0.1.0-alpha.1`.
   The Release workflow builds the DMG, engine packs, CLI binaries, homelab
   image, engine sources and `SHA256SUMS.txt` into a **draft** release.
4. Check the draft: the DMG opens on a clean Mac, an on-demand engine
   downloads (try OCR), and `SHA256SUMS.txt` matches. If signed, run
   `spctl -a -vv /Applications/Dogubako.app` (it should say "Notarized Developer ID").
5. Paste `docs/releases/v0.1.0-alpha.1.md` (below its line) into the release
   body, delete the "Opening the app" variant that doesn't apply, tick
   **Set as a pre-release**, and publish.
6. Make the `dogubako` package public: GitHub → Packages → dogubako →
   Package settings → Change visibility.
7. Deploy the website: Actions → Website → Run workflow with the chosen
   concept, `docs_live` ticked if docs.dogubako.app is up, release
   `v0.1.0-alpha.1`, `deploy` ticked. Its summary lists any copy that still
   says "not released yet"; edit that copy on the concept first (the link
   script only changes links).
8. Announce.

## Rollback

- Bad release: mark it as a draft again (Edit release → Save as draft), fix,
  and tag `v0.1.0-alpha.2`. Don't move a published tag.
- Bad website deploy: Cloudflare Pages → dogubako-website → Deployments →
  roll back to the previous one.

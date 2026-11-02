---
title: The docs site
description: How docs.dogubako.app is built, how to edit it, regenerate the tool catalog, and deploy it to Cloudflare Pages.
sidebar:
  order: 7
---

This site lives in `site/` in the repository. It is built with
[Astro Starlight](https://starlight.astro.build) into plain static files and
hosted on Cloudflare Pages at `docs.dogubako.app`.

## Working on it

```sh
cd site
npm ci
npm run dev          # http://localhost:4321, reloads as you edit
npm run build        # → site/dist, and checks every internal link and anchor
npm run preview      # serve the built site
```

Pages are Markdown (or MDX) files in `src/content/docs/`. The sidebar order is
set in `astro.config.mjs`. The site's look, matching the app's paper theme,
is in `src/styles/theme.css`.

Writing conventions:

- Write for someone who wants to get a job done. Lead with the task and the
  command, then explain.
- Name UI elements exactly as the app shows them, in bold: **Settings →
  Dependencies**.
- Link with absolute paths and a trailing slash: `[Docker](/self-hosting/docker/)`.
  The build fails on broken links and anchors.
- Check commands against the real CLI before you publish them.

## Generated pages

The [tool catalog](/tools/), the engine and model tables on
[Engines and models](/app/engines-and-models/), and [`/tools.json`](/tools.json)
are generated from the real tool registry, so they can't drift from the code.
Regenerate them whenever tools, engines or models change:

```sh
cargo build -p dogubako-cli          # from the repository root
cd site && npm run gen:tools         # uses ../target/{release,debug}/dogubako
```

Set `DOGUBAKO_BIN` to use another binary. The script writes
`src/content/docs/tools/*.md`, `src/data/*.json` and `public/tools.json`;
don't edit those by hand.

Screenshots come from `docs/screenshots/` in the repository. After they are
refreshed, copy them in with `npm run gen:screenshots`.

## Deploying to Cloudflare Pages

The build output is static (`site/dist`), with Cloudflare's `_headers` and
`_redirects` files included. There are three ways to deploy; pick one.

### Option 1: Cloudflare's Git integration

1. In the Cloudflare dashboard, go to **Workers & Pages → Create → Pages →
   Connect to Git** and pick the repository.
2. Set:
   - **Production branch:** `main`
   - **Root directory:** `site`
   - **Build command:** `npm ci && npm run build`
   - **Build output directory:** `dist`
   - **Environment variable:** `NODE_VERSION` = `22`
3. Save and deploy. Every push to `main` deploys, and pull requests get
   preview URLs.

### Option 2: GitHub Actions

`.github/workflows/docs.yml` builds the site on every pull request that
touches `site/`. On pushes to `main` it also deploys, once two repository
secrets exist:

- `CLOUDFLARE_API_TOKEN`: an API token with the **Cloudflare Pages: Edit**
  permission;
- `CLOUDFLARE_ACCOUNT_ID`: your account id, shown in the dashboard sidebar.

Create the Pages project once first, with
`npx wrangler pages project create dogubako-docs --production-branch main`.
Without the secrets, the deploy job is skipped.

### Option 3: From your machine

```sh
cd site
npx wrangler login
npx wrangler pages project create dogubako-docs --production-branch main   # first time only
npm run deploy
```

### The custom domain

In the Pages project, go to **Custom domains → Set up a custom domain** and
enter `docs.dogubako.app`. If `dogubako.app` is on Cloudflare DNS, the record
is created for you; otherwise add a `CNAME` from `docs` to
`dogubako-docs.pages.dev`. HTTPS certificates are automatic.

## What's in the box

| File | Purpose |
|---|---|
| `astro.config.mjs` | Site URL, sidebar, theme, link checking |
| `src/content/docs/` | The pages |
| `src/components/` | Tables rendered from generated data |
| `src/styles/theme.css` | Colors and type matching the app |
| `scripts/gen-tools.mjs` | Generates the catalog from the registry |
| `scripts/copy-screenshots.mjs` | Copies `docs/screenshots` into the site |
| `public/_headers` | Security and caching headers for Cloudflare Pages |
| `public/_redirects` | Short links such as `/cli` and `/api` |
| `wrangler.toml` | The Pages project name and output folder for `wrangler` |

Search is built in (Pagefind) and works offline in the static build. A
sitemap is generated at `/sitemap-index.xml`.

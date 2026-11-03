# Dogubako docs site

The source of <https://docs.dogubako.app>, built with
[Astro Starlight](https://starlight.astro.build) into static files for
Cloudflare Pages.

```sh
npm ci
npm run dev            # http://localhost:4321
npm run build          # → dist/, fails on broken links
npm run gen:tools      # regenerate the tool catalog from ../target/*/dogubako
npm run gen:screenshots
npm run deploy         # wrangler pages deploy (after `npx wrangler login`)
```

## Deploying

Cloudflare Pages settings for the Git integration:

| Setting | Value |
|---|---|
| Root directory | `site` |
| Build command | `npm ci && npm run build` |
| Build output directory | `dist` |
| Environment variable | `NODE_VERSION=22` |
| Custom domain | `docs.dogubako.app` |

Or let GitHub Actions deploy on pushes to `main` by adding the
`CLOUDFLARE_API_TOKEN` and `CLOUDFLARE_ACCOUNT_ID` repository secrets
(see `.github/workflows/docs.yml`).

The full guide is on the site itself, at
[/develop/docs-site/](https://docs.dogubako.app/develop/docs-site/)
(`src/content/docs/develop/docs-site.md`).

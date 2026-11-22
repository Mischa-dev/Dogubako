# Official tool store

Reviewed extension manifests, one folder per tool: `store/<id>/manifest.toml`
(the folder name is the tool id). They are not bundled; users install them with
`dogubako store install <id>` (or from Settings → Tool store).

* Every manifest must have `[[tests]]` that pass offline (record HTTP
  responses with `http = {...}`, program output with `command = {...}`); CI
  runs `dogubako store test`.
* Bump `version` when you change a manifest, then rebuild the published index
  and commit it:

  ```
  cargo run -p dogubako-cli -- store build   # writes site/public/store/
  ```

  CI fails when `site/public/store/` is stale (`dogubako store build --check`).
* The index is published with the docs site at
  https://docs.dogubako.app/store/index.json. Installs refuse a manifest whose
  SHA-256 differs from its index entry.

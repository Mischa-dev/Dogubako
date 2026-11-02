# Example extensions

Declarative TOML manifests that wrap command-line programs as Dogubako tools —
no Rust required. See [docs/TOOL_DEVELOPMENT.md](../docs/TOOL_DEVELOPMENT.md#declarative-extensions).

```sh
cp extensions/cwebp.toml ~/.local/share/dogubako/extensions/     # Linux per-user folder
dogubako extensions list
dogubako extensions trust image.cwebp      # pins the manifest's SHA-256
dogubako image cwebp photo.png --quality 90
```

Server: put manifests in `<data_dir>/extensions/` and run
`dogubako extensions --dir <data_dir>/extensions trust <id>`, then restart.
Editing a trusted manifest invalidates its trust until re-approved.

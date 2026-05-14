# Icons

Drop your icon files here. Tauri needs these formats (paths referenced in `tauri.conf.json`):

- `32x32.png`
- `128x128.png`
- `128x128@2x.png`
- `icon.ico` (Windows)
- `icon.icns` (macOS)

Generate all formats from a single 1024×1024 PNG with:

```bash
pnpm tauri icon path/to/source.png
```

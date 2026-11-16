# Folio

A calm news reader, written entirely in Rust — the window is drawn by [egui](https://github.com/emilk/egui), there is no web view, no Electron, no bundler. Ships as one binary.

## Features

- **Feeds** — add RSS/Atom feeds by URL, grouped by category, fetched in parallel
- **All stories / Saved** — save any story for later; saved items survive the cache prune
- **Search** — full-text match over titles, excerpts and source names across the whole cache
- **Feed health** — each feed shows its last success time; dead feeds get a red dot and a Retry button
- **OPML import/export** plus a JSON backup format
- **Share** — copy any story link to the clipboard
- **Update check** — against this repo's latest GitHub release
- **Rolling 600-story cache** in SQLite (WAL), stored in the user config dir

## Build

```
cargo build --release
# artifact: target/release/folio(.exe)
```

## Data

Windows: `%APPDATA%\Folio` · Linux/macOS: `~/.config/Folio`
Override with the `FOLIO_DATA_DIR` environment variable.

## Updates

The Manage feeds footer checks `releases/latest` on GitHub. Releases are tagged `vX.Y.Z`; CI builds the Windows and Linux binaries and publishes them with SHA-256 checksums.

## Versioning

SemVer from 0.1.0. Pre-1.0 releases may break; 1.0.0 will mark feature parity with a stable format.

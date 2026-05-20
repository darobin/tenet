# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

Tenet is a viewer for a better document format.

## Project Definition

This project is a Tauri v2 application with the following characteristics:

- It works like a typical PDF viewer, albeit on a different file format (tiles).
- It must cross-compile for macOS, Linux, Windows, Android, and iOS.
- It must register as a handler with the OS for files with extension `*.tile` or media type
  `application/tile`. When such a file is opened through typical OS means, this app is the
  one that gets to open the file (at least if it has been installed).
- Tile files are CAR containers as defined in the Web Tiles specification at https://dasl.ing/tiles.html.
- A good Rust implementation of CAR is `iroh-car`, a good example of using CAR in code is
  https://github.com/blacksky-algorithms/rsky/blob/main/rsky-satnav/.
- The way in which a tile file is opened is as follows:
  1. The file is in CAR format, the CAR is parsed. The MASL from the CAR header is extracted and the `resources` entry in
     the MASL is the resource map. Keys in the resource map are /-rooted paths into the tile and values contain both
     the CID of each resource under `src` and HTTP headers. The CID of each resource corresponds to an entry in the CAR.
     Keep track of the byte offset of the data of each entry keyed by CID so that entries in the resource map can be loaded
     by seeking into the CAR and reading just that entry.
  2. The Tauri backend exposes the content of that tile using a `tile:` custom protocol in which the authority is derived from
     the file name and the path is the key into `resources`.
  3. The Tauri frontend receives a message to show the tile and sets up to show the tile in the UI. The message also contains
     the full MASL metadata from the CAR header.
  4. The frontend is coded in JavaScript using custom elements built with the lit framework and reactive data sources driven
     by https://www.npmjs.com/package/refrakt.
  5. The UI is tab-based. Showing a new tile involves creating a new tab and then creating an `iframe` inside of that tab
     that points to the `tile:` URI. This triggers loading the content from the custom protocol.
  6. Each tab has the first icon from the `icons` field of the MASL and a text label from the `name` field of the MASL. It
     also has a close button.

## Self-Modifying Tiles

Tiles can support self-modification in the following way:

- A module exposing the API to the tile can be loaded from `/.well-known/web-tiles/store.js`.
- Note that all paths under `/.well-known/web-tiles/` can never be contained in the tile
  itself and aren't in `resources`. They are special paths that get provided by the context
  implementation, in this case the app.
- The API is basically `async loadData(name)` and `async putData(name, data)` where the data that is
  put and returned is whatever transferrable binary data works.
- The data in question is read/written from a special path in `resources` which is
  `/.well-known/web-tiles-storage/name` where `name` is the key provided in the API.
- When loading data, if `name` doesn't exist you simply get a 404.
- When putting data, that path gets added (if it doesn't exist) or updated (with the right CID) in
  `resources` and the data itself stored in the CAR. If possible, that CAR entry should be at the
  end of the CAR file. Any previous CAR entry for that `name` should be deleted from the CAR.
  The CAR is then persisted to disk in place.

## Commands

```sh
npm install                      # install frontend deps
npm run tauri dev                # run dev build (starts Vite + Tauri)
npm run tauri build              # production build

# Rust only (from repo root or src-tauri/)
cargo build                      # build backend
cargo test                       # run all tests
cargo test <name>                # run a single test
cargo clippy                     # lint
cargo fmt                        # format
```

## Architecture

```
tenet/
├── index.html            # Vite entry point
├── src/
│   ├── main.js           # Root <tile-app> element; listens for tile:opened events
│   ├── state.js          # refrakt store (tabs[], activeIndex)
│   └── components/
│       ├── tab-bar.js    # <tile-tab-bar>: tab strip + "Open" button
│       └── tile-tab.js   # <tile-content>: iframes for each open tile
└── src-tauri/
    ├── tauri.conf.json   # app config, file associations (.tile / application/tile)
    ├── capabilities/     # Tauri v2 permission declarations
    └── src/
        ├── main.rs       # calls lib::run()
        ├── lib.rs        # Tauri builder: tile: protocol, open_tile command, deep-link setup
        └── car.rs        # CAR v1 parser + MASL extraction
```

### Data flow

1. A `.tile` file is opened (CLI arg, OS file-open, or dialog).
2. `car::parse_tile()` reads the file: decodes the CBOR header to extract **MASL** (name, resources map, icons), then walks all CAR blocks recording each block's **byte offset** in the file keyed by CID.
3. The tile is stored in `TileStore` (authority → `TileContent`) and a `tile:opened` event is emitted to the frontend with `{ authority, masl }`.
4. The frontend's `state.js` (refrakt store) appends a new tab; `<tile-tab-bar>` renders the tab using `masl.name` and `masl.icons[0]`; `<tile-content>` shows an `<iframe src="tile://<authority>/">`.
5. The `tile:` URI scheme handler in `lib.rs` resolves each request: looks up the URL path in `masl.resources`, seeks to the stored offset in the file, reads the block bytes, and returns them with the headers declared in the resource entry.

### Key conventions

- **Authority**: derived from the tile filename stem (lowercased, non-alphanumeric → `-`).
- **MASL CID links** in the header CBOR are DAG-CBOR Tag(42, bytes) where the first byte is the identity-multibase prefix `0x00` followed by raw CID bytes.
- **Lit + refrakt**: components extend `SignalWatcher(LitElement)` from `@lit-labs/signals` so they re-render automatically when the refrakt store's TC39 signal updates.

## License

Apache 2.0

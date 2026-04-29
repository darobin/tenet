mod car;

use car::{authority_from_path, parse_tile, write_tile_data, Masl, TileContent};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::menu::{MenuBuilder, MenuItemBuilder, SubmenuBuilder};
use tauri::{AppHandle, Emitter, Listener, Manager, State};
use tauri_plugin_window_state::{Builder as WindowStateBuilder, StateFlags, WindowExt};

// ── Shared state ─────────────────────────────────────────────────────────────

struct TileStoreInner {
    map: HashMap<String, TileContent>,
    /// Open paths in insertion order — drives session save/restore.
    paths: Vec<PathBuf>,
}

struct TileStore(Mutex<TileStoreInner>);

// ── Frontend-facing types ────────────────────────────────────────────────────

/// Sent to the frontend when a tile is opened (via command or file-open event).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TileOpenedPayload {
    pub authority: String,
    pub masl: Masl,
}

// ── Commands ─────────────────────────────────────────────────────────────────

/// Open a `.tile` file at the given path, load it into the store, and emit
/// `tile:opened`. The frontend should navigate to `tile://<authority>/`.
#[tauri::command]
fn open_tile(
    path: String,
    state: State<'_, TileStore>,
    app: AppHandle,
) -> Result<TileOpenedPayload, String> {
    load_tile(&PathBuf::from(&path), &state, &app).map_err(|e| e.to_string())
}

/// Return all currently-open tiles in open order. Called by the frontend once
/// on startup to populate tabs that were restored from the previous session or
/// provided as CLI arguments (whose events may have fired before the webview
/// was ready to receive them).
#[tauri::command]
fn get_open_tiles(state: State<'_, TileStore>) -> Vec<TileOpenedPayload> {
    let store = state.0.lock().unwrap();
    store
        .paths
        .iter()
        .filter_map(|path| {
            let authority = authority_from_path(path);
            store.map.get(&authority).map(|content| TileOpenedPayload {
                authority,
                masl: content.masl.clone(),
            })
        })
        .collect()
}

/// Remove a tile from the store when its tab is closed in the frontend, so
/// that the session file stays accurate.
#[tauri::command]
fn close_tile(authority: String, state: State<'_, TileStore>, app: AppHandle) {
    {
        let mut store = state.0.lock().unwrap();
        store.map.remove(&authority);
        store.paths.retain(|p| authority_from_path(p) != authority);
    }
    save_session(&app);
}

// ── Internal helpers ─────────────────────────────────────────────────────────

/// Parse a tile, insert it into the store (skipping duplicates), and emit
/// `tile:opened` so the frontend can add a new tab.
fn load_tile(
    path: &Path,
    state: &State<'_, TileStore>,
    app: &AppHandle,
) -> anyhow::Result<TileOpenedPayload> {
    let content = parse_tile(path)?;
    let authority = authority_from_path(path);
    let payload = TileOpenedPayload { authority: authority.clone(), masl: content.masl.clone() };
    {
        let mut store = state.0.lock().unwrap();
        if !store.map.contains_key(&authority) {
            store.paths.push(path.to_path_buf());
            store.map.insert(authority, content);
        }
    }
    app.emit("tile:opened", &payload)?;
    save_session(app);
    Ok(payload)
}

// ── Session persistence ───────────────────────────────────────────────────────

fn session_file<R: tauri::Runtime>(app: &AppHandle<R>) -> Option<PathBuf> {
    app.path().app_data_dir().ok().map(|d| d.join("session.json"))
}

/// Write the ordered list of open tile paths to `session.json`.
fn save_session<R: tauri::Runtime>(app: &AppHandle<R>) {
    let store = app.state::<TileStore>();
    let paths: Vec<String> = store
        .0
        .lock()
        .unwrap()
        .paths
        .iter()
        .filter_map(|p| p.to_str().map(str::to_owned))
        .collect();
    if let Some(file) = session_file(app) {
        if let Some(parent) = file.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(&file, serde_json::to_string(&paths).unwrap_or_default());
    }
}

/// Read `session.json` and silently load any tile files that still exist.
/// Does not emit events — the frontend calls `get_open_tiles` after loading.
fn restore_session<R: tauri::Runtime>(app: &AppHandle<R>) {
    let Some(file) = session_file(app) else { return };
    let Ok(data) = std::fs::read_to_string(&file) else { return };
    let Ok(paths) = serde_json::from_str::<Vec<String>>(&data) else { return };

    let store = app.state::<TileStore>();
    for path_str in paths {
        let path = PathBuf::from(&path_str);
        if !path.exists() {
            continue;
        }
        if let Ok(content) = parse_tile(&path) {
            let authority = authority_from_path(&path);
            let mut s = store.0.lock().unwrap();
            if !s.map.contains_key(&authority) {
                s.paths.push(path);
                s.map.insert(authority, content);
            }
        }
    }
}

// ── tile: custom protocol ─────────────────────────────────────────────────────

/// JS module served at `tile://<authority>/.well-known/web-tiles/store.js`.
/// Tiles import this to read and write their self-modifiable storage area.
const STORE_JS: &str = r#"/**
 * Load data stored under `name`. Returns an ArrayBuffer, or null if the
 * key has never been written.
 */
export async function loadData(name) {
  const url = `/.well-known/web-tiles-storage/${encodeURIComponent(name)}`;
  const res = await fetch(url);
  if (res.status === 404) return null;
  if (!res.ok) throw new Error(`loadData failed: ${res.statusText}`);
  return await res.arrayBuffer();
}

/**
 * Persist `data` (ArrayBuffer or typed array) under `name`.
 * The tile's CAR file is rewritten in-place by the app.
 */
export async function putData(name, data) {
  const url = `/.well-known/web-tiles-storage/${encodeURIComponent(name)}`;
  const res = await fetch(url, { method: 'PUT', body: data });
  if (!res.ok) throw new Error(`putData failed: ${res.statusText}`);
}
"#;

fn handle_tile_protocol(
    app: &AppHandle<impl tauri::Runtime>,
    request: tauri::http::Request<Vec<u8>>,
) -> tauri::http::Response<Vec<u8>> {
    let uri = request.uri();
    let authority = uri.host().unwrap_or("").to_owned();
    let raw_path = uri.path();
    let path = if raw_path.is_empty() { "/".to_owned() } else { raw_path.to_owned() };

    let make_error = |status: u16, msg: &str| {
        tauri::http::Response::builder()
            .status(status)
            .header("content-type", "text/plain")
            .body(msg.as_bytes().to_vec())
            .unwrap()
    };

    // ── Static app-provided modules ───────────────────────────────────────
    if let Some(filename) = path.strip_prefix("/.well-known/web-tiles/") {
        return match filename {
            "store.js" => tauri::http::Response::builder()
                .status(200)
                .header("content-type", "text/javascript")
                .body(STORE_JS.as_bytes().to_vec())
                .unwrap(),
            _ => make_error(404, "unknown well-known resource"),
        };
    }

    let store = app.state::<TileStore>();

    // ── PUT: self-modifying tile storage ──────────────────────────────────
    if request.method().as_str() == "PUT" {
        let name = match path.strip_prefix("/.well-known/web-tiles-storage/") {
            Some(n) if !n.is_empty() => n.to_owned(),
            _ => return make_error(400, "PUT only allowed under /.well-known/web-tiles-storage/"),
        };
        let body = request.body().clone();
        let mut guard = store.0.lock().unwrap();
        let tile = match guard.map.get_mut(&authority) {
            Some(t) => t,
            None => return make_error(404, "tile not loaded"),
        };
        return match write_tile_data(tile, &name, body) {
            Ok(()) => tauri::http::Response::builder()
                .status(204)
                .body(Vec::new())
                .unwrap(),
            Err(e) => make_error(500, &e.to_string()),
        };
    }

    // ── GET: serve tile resources ─────────────────────────────────────────
    let guard = store.0.lock().unwrap();

    let tile = match guard.map.get(&authority) {
        Some(t) => t,
        None => return make_error(404, "tile not loaded"),
    };

    // Try exact path, then with/without trailing slash, then /index.html fallback.
    let with_slash = format!("{path}/");
    let candidates: &[&str] = &[
        &path,
        if path.ends_with('/') { path.trim_end_matches('/') } else { &path },
        if !path.ends_with('/') { &with_slash } else { &path },
        if path == "/" { "/index.html" } else { &path },
    ];

    let resource = match candidates.iter().find_map(|p| tile.masl.resources.get(*p)) {
        Some(r) => r,
        None => return make_error(404, &format!("no resource at {path}")),
    };

    let src = match resource.get("src") {
        Some(s) => s.as_str(),
        None => return make_error(500, "resource missing src"),
    };
    let data = match tile.read_block(src) {
        Ok(d) => d,
        Err(e) => return make_error(500, &e.to_string()),
    };

    let content_type = resource
        .get("content-type")
        .cloned()
        .unwrap_or_else(|| "application/octet-stream".to_string());

    let mut builder = tauri::http::Response::builder()
        .status(200)
        .header("content-type", &content_type)
        .header("access-control-allow-origin", "*");

    for (k, v) in resource {
        if k != "content-type" && k != "src" {
            builder = builder.header(k.as_str(), v.as_str());
        }
    }

    builder.body(data).unwrap()
}

// ── App entry point ───────────────────────────────────────────────────────────

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(WindowStateBuilder::new().build())
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .manage(TileStore(Mutex::new(TileStoreInner {
            map: HashMap::new(),
            paths: Vec::new(),
        })))
        .register_uri_scheme_protocol("tile", |ctx, request| {
            handle_tile_protocol(ctx.app_handle(), request)
        })
        .invoke_handler(tauri::generate_handler![open_tile, get_open_tiles, close_tile])
        .menu(|app| {
            let mut builder = MenuBuilder::new(app);

            let open_item = MenuItemBuilder::with_id("open_file", "Open…")
                .accelerator("CmdOrCtrl+O")
                .build(app)?;
            let file_menu = SubmenuBuilder::new(app, "File").item(&open_item).build()?;

            #[cfg(target_os = "macos")]
            {
                use tauri::menu::PredefinedMenuItem;
                let app_menu = SubmenuBuilder::new(app, "Tile Documents")
                    .item(&PredefinedMenuItem::about(app, None, None)?)
                    .separator()
                    .item(&PredefinedMenuItem::services(app, None)?)
                    .separator()
                    .item(&PredefinedMenuItem::hide(app, None)?)
                    .item(&PredefinedMenuItem::hide_others(app, None)?)
                    .item(&PredefinedMenuItem::show_all(app, None)?)
                    .separator()
                    .item(&PredefinedMenuItem::quit(app, None)?)
                    .build()?;
                let view = SubmenuBuilder::new(app, "View")
                    .item(&PredefinedMenuItem::fullscreen(app, None)?)
                    .build()?;
                builder = builder.item(&app_menu).item(&file_menu).item(&view);
            }

            #[cfg(not(target_os = "macos"))]
            {
                let toggle_fs =
                    MenuItemBuilder::with_id("toggle_fullscreen", "Toggle Full Screen")
                        .accelerator("F11")
                        .build(app)?;
                let view = SubmenuBuilder::new(app, "View").item(&toggle_fs).build()?;
                builder = builder.item(&file_menu).item(&view);
            }

            builder.build()
        })
        .on_menu_event(|app, event| {
            match event.id().as_ref() {
                "open_file" => {
                    let _ = app.emit("menu:open-file", ());
                }
                "toggle_fullscreen" => {
                    if let Some(window) = app.get_webview_window("main") {
                        let is_fs = window.is_fullscreen().unwrap_or(false);
                        let _ = window.set_fullscreen(!is_fs);
                    }
                }
                _ => {}
            }
        })
        .setup(|app| {
            let app_handle = app.handle().clone();

            // Restore the previous session before anything else, so that
            // get_open_tiles() returns the full set when the frontend loads.
            restore_session(&app_handle);

            // Restore window geometry (position, size, fullscreen).
            if let Some(window) = app_handle.get_webview_window("main") {
                let _ = window.restore_state(StateFlags::all());

                let app_for_event = app_handle.clone();
                let last_fs = Arc::new(AtomicBool::new(
                    window.is_fullscreen().unwrap_or(false),
                ));
                window.on_window_event(move |event| {
                    match event {
                        tauri::WindowEvent::Resized(_) => {
                            if let Some(win) = app_for_event.get_webview_window("main") {
                                let is_fs = win.is_fullscreen().unwrap_or(false);
                                let prev = last_fs.swap(is_fs, Ordering::Relaxed);
                                if prev != is_fs {
                                    let _ =
                                        app_for_event.emit("tile:fullscreen-changed", is_fs);
                                }
                            }
                        }
                        tauri::WindowEvent::CloseRequested { .. } => {
                            save_session(&app_for_event);
                        }
                        _ => {}
                    }
                });
            }

            // Files passed as CLI arguments (Windows / Linux).
            let state = app_handle.state::<TileStore>();
            for arg in std::env::args().skip(1) {
                let p = PathBuf::from(&arg);
                if p.extension().and_then(|e| e.to_str()) == Some("tile") && p.exists() {
                    let _ = load_tile(&p, &state, &app_handle);
                }
            }

            // macOS / iOS file-open via deep-link.
            #[cfg(any(target_os = "macos", target_os = "ios"))]
            {
                let app_handle2 = app_handle.clone();
                app.listen("deep-link://new-url", move |event| {
                    if let Ok(urls) = serde_json::from_str::<Vec<String>>(event.payload()) {
                        let state = app_handle2.state::<TileStore>();
                        for url in urls {
                            if let Some(file_path) = url.strip_prefix("file://") {
                                let p = PathBuf::from(file_path);
                                let _ = load_tile(&p, &state, &app_handle2);
                            }
                        }
                    }
                });
            }

            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error building Tile Documents")
        .run(|app_handle, event| {
            // Save session on every exit path (covers Cmd+Q, window close,
            // and any other shutdown route).
            if let tauri::RunEvent::Exit = event {
                save_session(app_handle);
            }
        });
}

mod car;

use car::{
    authority_from_path, flush_header, parse_tile, safe_model_stem, write_model_tile,
    write_tile_data, Masl, TileContent,
};
use tauri_plugin_dialog::DialogExt;
use unicode_segmentation::UnicodeSegmentation as _;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::menu::{MenuBuilder, MenuItemBuilder, SubmenuBuilder};
use tauri::{AppHandle, Emitter, Listener, Manager, State};
use tauri_plugin_window_state::Builder as WindowStateBuilder;

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
    /// Platform-correct base URL for the tile iframe: `tile://<authority>/` on
    /// macOS/Linux, `https://tile.<authority>/` on Windows (WRY workaround).
    pub url: String,
    pub masl: Masl,
}

/// A model/template in the user's library. `url` is the `tile:` origin under
/// which the model's resources (e.g. its icon) can be loaded by the frontend.
#[derive(Debug, Clone, Serialize)]
pub struct ModelEntry {
    pub id: String,
    pub authority: String,
    pub url: String,
    pub masl: Masl,
}

fn tile_origin(authority: &str) -> String {
    // On Windows, WRY's WebView2 workaround requires the host to be `localhost`
    // (the only resolvable special hostname). The authority goes in the first
    // path segment: https://tile.localhost/<authority>/
    // On macOS/Linux the authority is the host: tile://<authority>/
    #[cfg(windows)]
    return format!("https://tile.{authority}.localhost/");
    #[cfg(not(windows))]
    return format!("tile://{authority}/");
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
                url: tile_origin(&authority),
                authority,
                masl: content.masl.clone(),
            })
        })
        .collect()
}

#[tauri::command]
fn set_fullscreen(window: tauri::WebviewWindow, fullscreen: bool) {
    let _ = window.set_fullscreen(fullscreen);
}

#[tauri::command]
fn set_title(authority: Option<String>, window: tauri::WebviewWindow, state: State<'_, TileStore>) {
    match authority.as_deref().filter(|a| !a.is_empty()) {
        None => { let _ = window.set_title("Tenet"); }
        Some(auth) => {
            let store = state.0.lock().unwrap();
            let Some(tile) = store.map.get(auth) else { return };
            let _ = window.set_title(&tile.masl.name);
        }
    }
}

#[tauri::command]
fn set_tile_name(name: String, authority: String, state: State<'_, TileStore>) -> Result<(), String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err("name must not be empty".into());
    }
    if trimmed.graphemes(true).count() > 300 {
        return Err("name must not exceed 300 graphemes".into());
    }
    let mut store = state.0.lock().unwrap();
    let Some(tile) = store.map.get_mut(&authority) else {
        return Err("tile not found".into());
    };
    if tile.masl.model.is_none() {
        return Err("tile does not have a model field".into());
    }
    tile.masl.name = trimmed.to_owned();
    flush_header(tile).map_err(|e| e.to_string())
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

// ── Model library commands ──────────────────────────────────────────────────────

/// Add (or update) the currently-open tile identified by `authority` to the
/// model library. The tile must carry a `model` field with an `id`. The stored
/// model has its top-level metadata taken from `model`, its self-storage
/// stripped, and is keyed on disk by `model.id` — so re-adding the same id
/// overwrites it (i.e. this is an upsert / update).
#[tauri::command]
fn add_model(
    authority: String,
    state: State<'_, TileStore>,
    app: AppHandle,
) -> Result<ModelEntry, String> {
    let dest = {
        let store = state.0.lock().unwrap();
        let tile = store.map.get(&authority).ok_or("tile not loaded")?;
        let model = tile.masl.model.as_ref().ok_or("tile has no model field")?;
        let id = model.id.as_deref().filter(|s| !s.trim().is_empty()).ok_or("model has no id")?;
        let dest = model_file_path(&app, id).ok_or("no app data dir")?;
        write_model_tile(tile, &dest).map_err(|e| e.to_string())?;
        dest
    };
    let entry = load_model(&state, &dest).map_err(|e| e.to_string())?;
    emit_models(&app);
    Ok(entry)
}

/// Remove a model from the library by `id`. When `to_trash` is true the model
/// file is moved to the OS trash/recycle bin (restorable by the user) instead of
/// being permanently deleted.
#[tauri::command]
fn remove_model(
    id: String,
    to_trash: bool,
    state: State<'_, TileStore>,
    app: AppHandle,
) -> Result<(), String> {
    let path = model_file_path(&app, &id).ok_or("no app data dir")?;
    let authority = authority_from_path(&path);
    if path.exists() {
        if to_trash {
            trash::delete(&path).map_err(|e| e.to_string())?;
        } else {
            std::fs::remove_file(&path).map_err(|e| e.to_string())?;
        }
    }
    state.0.lock().unwrap().map.remove(&authority);
    emit_models(&app);
    Ok(())
}

/// List every model in the library with its metadata. Each entry is also loaded
/// into the store so its icon is reachable at `tile://<authority>/<icon>`.
#[tauri::command]
fn list_models(state: State<'_, TileStore>, app: AppHandle) -> Vec<ModelEntry> {
    collect_models(&state, &app)
}

/// Create a new tile from the model with the given `id`: prompt for a `.tile`
/// save location, copy the model there, and open it in a new tab. The save
/// dialog is non-blocking; the new tab appears via the usual `tile:opened`
/// event once the user picks a destination.
#[tauri::command]
fn create_tile_from_model(id: String, app: AppHandle) -> Result<(), String> {
    let model_path = model_file_path(&app, &id).ok_or("no app data dir")?;
    if !model_path.exists() {
        return Err(format!("no model with id {id}"));
    }
    let default_name = format!("{}.tile", safe_model_stem(&id));
    let app_for_cb = app.clone();
    app.dialog()
        .file()
        .add_filter("Tile Documents", &["tile"])
        .set_file_name(&default_name)
        .save_file(move |maybe_path| {
            let Some(file_path) = maybe_path else { return };
            let Ok(mut dest) = file_path.into_path() else { return };
            if dest.extension().and_then(|e| e.to_str()) != Some("tile") {
                dest.set_extension("tile");
            }
            if std::fs::copy(&model_path, &dest).is_err() {
                return;
            }
            let state = app_for_cb.state::<TileStore>();
            let _ = load_tile(&dest, &state, &app_for_cb);
        });
    Ok(())
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
    let payload = TileOpenedPayload { url: tile_origin(&authority), authority: authority.clone(), masl: content.masl.clone() };
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

// ── Model library ──────────────────────────────────────────────────────────────

/// Directory under the app data dir that holds the user's model/template tiles.
fn models_dir(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_data_dir().ok().map(|d| d.join("models"))
}

/// Deterministic on-disk path for the model with the given `id`.
fn model_file_path(app: &AppHandle, id: &str) -> Option<PathBuf> {
    models_dir(app).map(|d| d.join(format!("{}.tile", safe_model_stem(id))))
}

/// Parse a model tile from disk, (re)insert it into the store under its
/// authority — so the `tile:` protocol can serve its icon — and build a
/// `ModelEntry`. Model tiles live in `map` but never in `paths`, so they don't
/// become tabs or get written to the session.
fn load_model(state: &State<'_, TileStore>, path: &Path) -> anyhow::Result<ModelEntry> {
    let content = parse_tile(path)?;
    let id = content
        .masl
        .model
        .as_ref()
        .and_then(|m| m.id.clone())
        .ok_or_else(|| anyhow::anyhow!("model tile is missing `model.id`"))?;
    let authority = authority_from_path(path);
    let masl = content.masl.clone();
    {
        let mut store = state.0.lock().unwrap();
        store.map.insert(authority.clone(), content);
    }
    Ok(ModelEntry { id, url: tile_origin(&authority), authority, masl })
}

/// Read the whole library from disk into a sorted list of `ModelEntry`, loading
/// each model into the store so its icon is reachable over the `tile:` protocol.
fn collect_models(state: &State<'_, TileStore>, app: &AppHandle) -> Vec<ModelEntry> {
    let Some(dir) = models_dir(app) else { return Vec::new() };
    let Ok(entries) = std::fs::read_dir(&dir) else { return Vec::new() };
    let mut models: Vec<ModelEntry> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|x| x.to_str()) == Some("tile"))
        .filter_map(|p| load_model(state, &p).ok())
        .collect();
    models.sort_by(|a, b| a.masl.name.to_lowercase().cmp(&b.masl.name.to_lowercase()));
    models
}

/// Emit the current model library to the frontend as `models:changed`. Sent on
/// launch (and webview reloads) and after every add/update/removal, so the
/// frontend can keep its model list in sync without polling.
fn emit_models(app: &AppHandle) {
    let state = app.state::<TileStore>();
    let models = collect_models(&state, app);
    let _ = app.emit("models:changed", &models);
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
    // On Windows, WRY reverts https://tile.<authority>.localhost/path back to
    // tile://<authority>.localhost/path before calling this handler.
    // Strip .localhost to recover the bare authority; path is unchanged.
    #[cfg(windows)]
    let (authority, path) = {
        let host = uri.host().unwrap_or("");
        let auth = host.strip_suffix(".localhost").unwrap_or(host).to_owned();
        let p = uri.path();
        (auth, if p.is_empty() { "/".to_owned() } else { p.to_owned() })
    };
    #[cfg(not(windows))]
    let (authority, path) = {
        let p = uri.path();
        (uri.host().unwrap_or("").to_owned(), if p.is_empty() { "/".to_owned() } else { p.to_owned() })
    };

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
                .header("content-type", "application/javascript")
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
            _ => return make_error(405, "PUT only allowed under /.well-known/web-tiles-storage/"),
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
    let mut guard = store.0.lock().unwrap();

    let tile = match guard.map.get_mut(&authority) {
        Some(t) => t,
        None => return make_error(404, "tile not loaded"),
    };
    // Re-parse the CAR if it was modified externally since the last read.
    let _ = tile.refresh_if_stale();

    // Try exact path, then with/without trailing slash, then /index.html fallback.
    let with_slash = format!("{path}/");
    let candidates: &[&str] = &[
        &path,
        if path.ends_with('/') { path.trim_end_matches('/') } else { &path },
        if !path.ends_with('/') { &with_slash } else { &path },
        // if path == "/" { "/index.html" } else { &path },
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
        // .header("access-control-allow-origin", "*")
        ;

    // XXX need to list accepted headers
    for (k, v) in resource {
        if k != "content-type" && k != "src" {
            builder = builder.header(k.as_str(), v.as_str());
        }
    }

    builder = builder
        .header("content-security-policy", "\
            default-src 'self' blob: data:; \
            script-src 'self' blob: data: 'unsafe-inline' 'wasm-unsafe-eval'; \
            script-src-attr 'none'; \
            style-src 'self' blob: data: 'unsafe-inline'; \
            form-src 'self'; \
            manifest-src 'none'; \
            object-src 'none'; \
            base-uri 'none'; \
            sandbox allow-downloads \
                    allow-forms \
                    allow-modals \
                    allow-popups \
                    allow-popups-to-escape-sandbox \
                    allow-same-origin \
                    allow-scripts\
            ")
        .header("cross-origin-opener-policy", "same-origin")
        .header("cross-origin-resource-policy", "cross-origin")
        .header("origin-agent-cluster", "?1")
        .header("permissions-policy", "interest-cohort=(), browsing-topics=()")
        .header("referrer-policy", "no-referrer")
        .header("x-content-type-options", "nosniff")
        .header("x-dns-prefetch-control", "off")
    ;

    builder.body(data).unwrap()
}

// ── Window geometry ───────────────────────────────────────────────────────────

/// After `restore_state` reapplies the saved position and size, validate that
/// the window is actually on an available monitor and fits within it.
///
/// - If the window centre falls inside a known monitor: clamp size/position so
///   the whole window is visible on that monitor.
/// - If the window is entirely off-screen (monitor was unplugged or resolution
///   changed): centre the window on the primary monitor (or the first one),
///   clamping the size to fit.
///
/// Maximised and fullscreen windows are left untouched.
fn fix_window_geometry(window: &tauri::WebviewWindow) {
    if window.is_maximized().unwrap_or(false) || window.is_fullscreen().unwrap_or(false) {
        return;
    }
    let Ok(monitors) = window.available_monitors() else { return };
    if monitors.is_empty() { return; }
    let Ok(pos) = window.outer_position() else { return };
    let Ok(size) = window.outer_size() else { return };

    // Use the window centre to decide which monitor "owns" it.
    let cx = pos.x + size.width as i32 / 2;
    let cy = pos.y + size.height as i32 / 2;

    let on_screen = monitors.iter().find(|m| {
        let mp = m.position();
        let ms = m.size();
        cx >= mp.x
            && cx < mp.x + ms.width as i32
            && cy >= mp.y
            && cy < mp.y + ms.height as i32
    });

    // Determine the target monitor and whether we need to re-centre.
    let (mp, ms, needs_centre) = match on_screen {
        Some(m) => (*m.position(), *m.size(), false),
        None => {
            let fallback = window
                .primary_monitor()
                .ok()
                .flatten()
                .unwrap_or_else(|| monitors[0].clone());
            (*fallback.position(), *fallback.size(), true)
        }
    };

    // Clamp size to the target monitor.
    let w = size.width.min(ms.width);
    let h = size.height.min(ms.height);

    let (x, y) = if needs_centre {
        // Centre on the fallback monitor.
        (
            mp.x + (ms.width as i32 - w as i32) / 2,
            mp.y + (ms.height as i32 - h as i32) / 2,
        )
    } else {
        // Clamp position so the whole window stays inside its monitor.
        (
            pos.x.clamp(mp.x, mp.x + ms.width as i32 - w as i32),
            pos.y.clamp(mp.y, mp.y + ms.height as i32 - h as i32),
        )
    };

    if w != size.width || h != size.height {
        let _ = window.set_size(tauri::Size::Physical(tauri::PhysicalSize { width: w, height: h }));
    }
    if x != pos.x || y != pos.y {
        let _ = window.set_position(tauri::Position::Physical(tauri::PhysicalPosition { x, y }));
    }
}

// ── Window placement actions (menu commands) ────────────────────────────────────

/// A region of the current monitor's work area to snap the window into.
enum ScreenRegion {
    Fill,
    Left,
    Right,
    Top,
    Bottom,
}

/// macOS-style "Zoom": toggle the window between maximized and its previous size.
fn window_zoom(window: &tauri::WebviewWindow) {
    if window.is_maximized().unwrap_or(false) {
        let _ = window.unmaximize();
    } else {
        let _ = window.maximize();
    }
}

/// Resize/position the window to a region of the current monitor's *work area*
/// (the screen minus the menu bar, dock, and taskbar).
fn window_snap(window: &tauri::WebviewWindow, region: ScreenRegion) {
    let Some(monitor) = window.current_monitor().ok().flatten() else { return };
    let area = monitor.work_area();
    let (ax, ay) = (area.position.x, area.position.y);
    let (aw, ah) = (area.size.width, area.size.height);

    let (x, y, w, h) = match region {
        ScreenRegion::Fill => (ax, ay, aw, ah),
        ScreenRegion::Left => (ax, ay, aw / 2, ah),
        ScreenRegion::Right => (ax + (aw / 2) as i32, ay, aw - aw / 2, ah),
        ScreenRegion::Top => (ax, ay, aw, ah / 2),
        ScreenRegion::Bottom => (ax, ay + (ah / 2) as i32, aw, ah - ah / 2),
    };

    // A maximized window ignores manual size/position changes until restored.
    if window.is_maximized().unwrap_or(false) {
        let _ = window.unmaximize();
    }
    let _ = window.set_size(tauri::Size::Physical(tauri::PhysicalSize { width: w, height: h }));
    let _ = window.set_position(tauri::Position::Physical(tauri::PhysicalPosition { x, y }));
}

/// Dispatch a window menu command by id against the main window. Returns `true`
/// if the id was a window command (handled here), `false` otherwise.
fn handle_window_menu(app: &AppHandle, id: &str) -> bool {
    let Some(window) = app.get_webview_window("main") else {
        // Still report window-command ids as handled so they aren't misrouted.
        return id.starts_with("win_");
    };
    match id {
        "win_minimize" => { let _ = window.minimize(); }
        "win_zoom" => window_zoom(&window),
        "win_center" => { let _ = window.center(); }
        "win_fill" => window_snap(&window, ScreenRegion::Fill),
        "win_left" => window_snap(&window, ScreenRegion::Left),
        "win_right" => window_snap(&window, ScreenRegion::Right),
        "win_top" => window_snap(&window, ScreenRegion::Top),
        "win_bottom" => window_snap(&window, ScreenRegion::Bottom),
        _ => return false,
    }
    true
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
        .invoke_handler(tauri::generate_handler![open_tile, get_open_tiles, close_tile, set_fullscreen, set_title, set_tile_name, add_model, remove_model, list_models, create_tile_from_model])
        .menu(|app| {
            let mut builder = MenuBuilder::new(app);

            let open_item = MenuItemBuilder::with_id("open_file", "Open…")
                .accelerator("CmdOrCtrl+O")
                .build(app)?;
            let close_item = MenuItemBuilder::with_id("close_file", "Close")
                .accelerator("CmdOrCtrl+W")
                .build(app)?;
            let file_menu = SubmenuBuilder::new(app, "File")
                .item(&open_item)
                .item(&close_item)
                .build()?;

            #[cfg(target_os = "macos")]
            {
                use tauri::menu::PredefinedMenuItem;
                let app_menu = SubmenuBuilder::new(app, "Tenet")
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

                // Window menu: native Minimize plus Zoom and a Move & Resize set
                // (Fill / Center / halves) snapping to the screen's work area.
                let zoom = MenuItemBuilder::with_id("win_zoom", "Zoom").build(app)?;
                let fill = MenuItemBuilder::with_id("win_fill", "Fill")
                    .accelerator("Ctrl+Alt+F").build(app)?;
                let center = MenuItemBuilder::with_id("win_center", "Center").build(app)?;
                let left = MenuItemBuilder::with_id("win_left", "Left Half")
                    .accelerator("Ctrl+Cmd+Left").build(app)?;
                let right = MenuItemBuilder::with_id("win_right", "Right Half")
                    .accelerator("Ctrl+Cmd+Right").build(app)?;
                let top = MenuItemBuilder::with_id("win_top", "Top Half")
                    .accelerator("Ctrl+Cmd+Up").build(app)?;
                let bottom = MenuItemBuilder::with_id("win_bottom", "Bottom Half")
                    .accelerator("Ctrl+Cmd+Down").build(app)?;
                let window_menu = SubmenuBuilder::new(app, "Window")
                    .item(&PredefinedMenuItem::minimize(app, None)?)
                    .item(&zoom)
                    .separator()
                    .item(&fill)
                    .item(&center)
                    .separator()
                    .item(&left)
                    .item(&right)
                    .item(&top)
                    .item(&bottom)
                    .build()?;

                builder = builder
                    .item(&app_menu)
                    .item(&file_menu)
                    .item(&view)
                    .item(&window_menu);
            }

            #[cfg(not(target_os = "macos"))]
            {
                let toggle_fs =
                    MenuItemBuilder::with_id("toggle_fullscreen", "Toggle Full Screen")
                        .accelerator("F11")
                        .build(app)?;
                let view = SubmenuBuilder::new(app, "View").item(&toggle_fs).build()?;

                // Window menu. The OS/WM already provides snapping (Super+Arrow),
                // so we offer the actions without conflicting accelerators.
                let minimize = MenuItemBuilder::with_id("win_minimize", "Minimize").build(app)?;
                let zoom = MenuItemBuilder::with_id("win_zoom", "Maximize").build(app)?;
                let fill = MenuItemBuilder::with_id("win_fill", "Fill")
                    .accelerator("Ctrl+Alt+F").build(app)?;
                let center = MenuItemBuilder::with_id("win_center", "Center").build(app)?;
                let window_menu = SubmenuBuilder::new(app, "Window")
                    .item(&minimize)
                    .item(&zoom)
                    .separator()
                    .item(&fill)
                    .item(&center)
                    .build()?;

                builder = builder.item(&file_menu).item(&view).item(&window_menu);
            }

            builder.build()
        })
        .on_menu_event(|app, event| {
            let id = event.id().as_ref();
            match id {
                "open_file" => {
                    let _ = app.emit("menu:open-file", ());
                }
                "close_file" => {
                    let _ = app.emit("menu:close-file", ());
                }
                "toggle_fullscreen" => {
                    if let Some(window) = app.get_webview_window("main") {
                        let is_fs = window.is_fullscreen().unwrap_or(false);
                        let _ = window.set_fullscreen(!is_fs);
                    }
                }
                _ => {
                    handle_window_menu(app, id);
                }
            }
        })
        .on_page_load(|webview, payload| {
            // Once the main window's document has loaded (initial launch and any
            // reload), push the current model library so the frontend can seed
            // and maintain its list from the `models:changed` event alone.
            use tauri::webview::PageLoadEvent;
            if webview.label() == "main" && payload.event() == PageLoadEvent::Finished {
                emit_models(webview.app_handle());
            }
        })
        .setup(|app| {
            let app_handle = app.handle().clone();

            // Restore the previous session before anything else, so that
            // get_open_tiles() returns the full set when the frontend loads.
            restore_session(&app_handle);

            // Window geometry (size, position, screen, fullscreen) is restored by
            // the window-state plugin itself, at `on_window_ready` — which fires
            // *after* this setup hook. We must NOT call `restore_state` here: doing
            // so runs before the window is realised, no-ops the resize, and clobbers
            // the plugin's cached geometry, leaving the window at its default size.
            //
            // Once the plugin has restored, clamp the window to the current monitor
            // (handles a saved size larger than the screen, or a vanished monitor).
            // We defer this briefly so it runs after the plugin's restore, and on the
            // main thread, where geometry calls take effect.
            if let Some(window) = app_handle.get_webview_window("main") {
                let win = window.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(400));
                    let win2 = win.clone();
                    let _ = win.run_on_main_thread(move || fix_window_geometry(&win2));
                });

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

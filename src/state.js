
import { store } from 'refrakt';
import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';

// ── Actions ──────────────────────────────────────────────────────────────────

export const ADD_TAB = 'ADD_TAB';
export const CLOSE_TAB = 'CLOSE_TAB';
export const ACTIVATE_TAB = 'ACTIVATE_TAB';
export const SET_FULLSCREEN = 'SET_FULLSCREEN';
export const SET_TILE_NAME = 'SET_TILE_NAME';
export const UPDATE_MODELS = 'UPDATE_MODELS';
export const SET_UI_STATE = 'SET_UI_STATE';
export const UPDATE_UI_STATE = 'UPDATE_UI_STATE';

// ── Reducer ───────────────────────────────────────────────────────────────────
function reducer (state, action) {
  switch (action.type) {
    case ADD_TAB: {
      // Ignore if this authority is already open (guards against race between
      // get_open_tiles and tile:opened events both delivering the same tile).
      const existsIndex = state.tabs.findIndex((t) => t.authority === action.tab.authority);
      if (existsIndex > -1) {
        return { ...state, activeIndex: existsIndex };
      }
      const tabs = [...state.tabs, action.tab];
      return { tabs, activeIndex: tabs.length - 1 };
    }
    case CLOSE_TAB: {
      const tabs = state.tabs.filter((_, i) => i !== action.index);
      const activeIndex = Math.min(
        state.activeIndex >= action.index ? state.activeIndex - 1 : state.activeIndex,
        tabs.length - 1,
      );
      return { tabs, activeIndex };
    }
    case ACTIVATE_TAB: {
      return { ...state, activeIndex: action.index };
    }
    case SET_FULLSCREEN: {
      return { ...state, fullscreen: action.fullscreen };
    }
    case SET_TILE_NAME: {
      let { tabs } = state;
      const targetIndex = tabs.findIndex((t) => t.authority === action.authority);
      if (targetIndex < 0) return;
      const newTabs = [...tabs];
      newTabs[targetIndex] = structuredClone(newTabs[targetIndex]);
      newTabs[targetIndex].masl.name = action.name;
      return { ...state, tabs: newTabs };
    }
    // This could be more subtle and only change the state for those that have actually changed
    case UPDATE_MODELS: {
      return { ...state, models: action.models };
    }
    case SET_UI_STATE: {
      return { ...state, uiState: action.uiState };
    }
    case UPDATE_UI_STATE: {
      let { uiState } = state;
      return { ...state, uiState: { ...uiState , [action.key]: action.value } };
    }
    default:
      return state;
  }
}

// ── Store ─────────────────────────────────────────────────────────────────────
export const appStore = store(reducer, {
  tabs: [],
  activeIndex: -1,
  fullscreen: false,
  models: [],
  uiState: {},
});
window.appStore = appStore;

// ── Interface ─────────────────────────────────────────────────────────────────
export async function addTab (authority, masl, url) {
  appStore.send({ type: ADD_TAB, tab: { authority, masl, url } });
  await invoke('set_title', { authority });
}

export function closeTab (index) {
  const { tabs } = appStore.get();
  const tab = tabs[index];
  appStore.send({ type: CLOSE_TAB, index });
  if (tab) invoke('close_tile', { authority: tab.authority });
}

export function closeActiveTab () {
  const { activeIndex } = appStore.get();
  return closeTab(activeIndex);
}

export async function activateTab (index) {
  appStore.send({ type: ACTIVATE_TAB, index });
  const { tabs } = appStore.get();
  const tab = tabs[index];
  await invoke('set_title', { authority: tab.authority });
}

export function setFullscreen (fullscreen) {
  appStore.send({ type: SET_FULLSCREEN, fullscreen });
}

export async function openTileDialog () {
  const filePath = await open({
    multiple: false,
    filters: [{ name: 'Tile Documents', extensions: ['tile'] }],
  });
  if (filePath) await invoke('open_tile', { path: filePath });
}

export async function setTileName (authority, name) {
  name = name.trim().replace(/\s{2,}/, ' ');
  if (name && name.length <= 300) {
    appStore.send({ type: SET_TILE_NAME, authority, name });
    await invoke('set_tile_name', { authority, name });
  }
}

export function updateModels (models) {
  appStore.send({ type: UPDATE_MODELS, models });
}

export function setUIState (uiState) {
  appStore.send({ type: SET_UI_STATE, uiState });
}
export async function updateUIState (key, value) {
  appStore.send({ type: UPDATE_UI_STATE, key, value });
  await invoke('set_ui_state', { key, value });
}
export async function openSidebar () {
  await updateUIState('sidebarOpen', true);
}
export async function closeSidebar () {
  await updateUIState('sidebarOpen', false);
}

// await invoke('remove_model', { id, toTrash: true });
// - add_model(authority) — turns the open tile into a library model. Requires model.id (errors otherwise). Keyed by id, so re-adding the same id overwrites it — this is your add and update.
// - remove_model(id) — deletes the file and drops it from the store.
// - list_models() -> [ModelEntry] — every model with { id, authority, url, masl }, sorted by name.
// - create_tile_from_model(id) — shows a native .tile save prompt (Rust-side, non-blocking), copies the model to the chosen path, and opens it in a new tab via the existing tile:opened flow.
//
// const entry = (await invoke('list_models'))[0];
// const iconUrl = new URL(entry.masl.icons[0].src, entry.url).href; // tile://…/icon.svg

// ── Helpers ───────────────────────────────────────────────────────────────────
export function activeTab () {
  const { tabs, activeIndex } = appStore.get();
  return tabs[activeIndex];
}

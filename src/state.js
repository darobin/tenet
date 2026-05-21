
import { store } from 'refrakt';
import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';

// ── Actions ──────────────────────────────────────────────────────────────────

export const ADD_TAB = 'ADD_TAB';
export const CLOSE_TAB = 'CLOSE_TAB';
export const ACTIVATE_TAB = 'ACTIVATE_TAB';
export const SET_FULLSCREEN = 'SET_FULLSCREEN';

// ── Reducer ───────────────────────────────────────────────────────────────────

function reducer (state, action) {
  switch (action.type) {
    case ADD_TAB: {
      // Ignore if this authority is already open (guards against race between
      // get_open_tiles and tile:opened events both delivering the same tile).
      const existsIndex = state.tabs.findIndex((t) => t.authority === action.tab.authority)
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
    default:
      return state;
  }
}

// ── Store ─────────────────────────────────────────────────────────────────────

export const appStore = store(reducer, { tabs: [], activeIndex: -1, fullscreen: false });
window.appStore = appStore;

// ── Helpers ───────────────────────────────────────────────────────────────────

export function addTab (authority, masl) {
  appStore.send({ type: ADD_TAB, tab: { authority, masl } });
}

export function closeTab (index) {
  const { tabs } = appStore.get();
  const tab = tabs[index];
  appStore.send({ type: CLOSE_TAB, index });
  if (tab) invoke('close_tile', { authority: tab.authority });
}

export function activateTab (index) {
  appStore.send({ type: ACTIVATE_TAB, index });
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

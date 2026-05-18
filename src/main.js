
import { LitElement, html, css, nothing } from 'lit';
import { SignalWatcher } from '@lit-labs/signals';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import '@supramundane/ui';
import '@supramundane/ui/tokens/light';
import { addTab, appStore, openTileDialog, setFullscreen } from './state.js';

import './components/tab-bar.js';
import './components/tile-tab.js';
import './components/toolbar.js';

// ── Root app shell ────────────────────────────────────────────────────────────

class TileApp extends SignalWatcher (LitElement) {
  static styles = css`
    :host {
      display: flex;
      flex-direction: column;
      height: 100vh;
      overflow: hidden;
    }
  `;

  connectedCallback () {
    super.connectedCallback();

    // Sync initial fullscreen state (e.g. restored from last session).
    getCurrentWindow()
      .isFullscreen()
      .then((isFs) => { if (isFs) setFullscreen(true); })
    ;

    // Populate tabs from session-restored tiles and any CLI-arg tiles that
    // were loaded before the webview was ready to receive events.
    invoke('get_open_tiles').then((tiles) => {
      for (const tile of tiles) addTab(tile.authority, tile.masl);
    });

    listen('tile:opened', (event) => {
      const { authority, masl } = event.payload;
      addTab(authority, masl);
    });

    listen('tile:fullscreen-changed', (event) => {
      setFullscreen(event.payload);
    });

    listen('menu:open-file', openTileDialog);
  }

  render () {
    const { fullscreen } = appStore.get();
    return html`
      <tile-toolbar></tile-toolbar>
      ${fullscreen ? nothing : html`<tile-tab-bar></tile-tab-bar>`}
      <tile-content></tile-content>
    `;
  }
}

customElements.define('tile-app', TileApp);

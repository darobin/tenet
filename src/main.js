
import { LitElement, html, css, nothing } from 'lit';
import { SignalWatcher } from '@lit-labs/signals';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import '@supramundane/ui/icon-button';
import '@supramundane/ui/tab-panel';
import '@supramundane/ui/tabbed-pane';
import '@supramundane/ui/toolbar';
import '@supramundane/ui/tokens/light';
import { folder2Open, fullscreen, arrowClockwise } from '@supramundane/ui/icons';
import '../css/arepo.css';
import { addTab, appStore, openTileDialog, setFullscreen, activateTab, closeTab, closeActiveTab } from './state.js';
import './el/model-header.js';


// ── Root app shell ────────────────────────────────────────────────────────────
class TileApp extends SignalWatcher (LitElement) {
  static styles = css`
    :host {
      display: flex;
      flex-direction: column;
      height: 100vh;
      overflow: hidden;
    }
    .empty {
      flex: 1;
      display: flex;
      align-items: center;
      justify-content: center;
      color: #555;
      font-size: 15px;
    }
    sm-tabbed-pane {
      flex-grow: 1;
    }
    iframe {
      height: 100%;
      width: 100%;
      border: none;
    }
  `;

  connectedCallback () {
    super.connectedCallback();
    getCurrentWindow()
      .isFullscreen()
      .then((isFs) => { if (isFs) setFullscreen(true); })
    ;
    invoke('get_open_tiles').then((tiles) => {
      console.warn(`get_open_tiles`, tiles);
      for (const tile of tiles) addTab(tile.authority, tile.masl, tile.url);
    });
    listen('tile:opened', (event) => {
      const { authority, masl, url } = event.payload;
      console.warn(`tile:opened`, authority, masl);
      addTab(authority, masl, url);
    });
    listen('tile:fullscreen-changed', (event) => {
      console.warn(`tile:fullscreen-changed`, event.payload);
      setFullscreen(event.payload);
    });
    listen('menu:open-file', openTileDialog);
    listen('menu:close-file', closeActiveTab);
  }

  #handleOpen (ev) {
    return openTileDialog();
  }
  #handleActivateTab (ev) {
    ev.preventDefault();
    activateTab(ev.detail.activeIndex);
  }
  #handleCloseTab (ev) {
    ev.preventDefault();
    closeTab(ev.detail.activeIndex);
    // activateTab(ev.detail.nextIndex); // XXX testing without, shouldn't be needed
  }
  #handleFullscreen (ev) {
    invoke('set_fullscreen', { fullscreen: true });
  }
  #handleReload () {
    const ifr = this.shadowRoot.querySelector('sm-tab-panel[active] iframe');
    if (!ifr) return;
    ifr.src = ifr.src;
  }

  render () {
    const { fullscreen: isFullscreen } = appStore.get();
    const { tabs, activeIndex } = appStore.get();
    return html`
      ${
        isFullscreen
        ? nothing
        : html`<sm-toolbar variant="flat">
            <sm-icon-button label="Open tile" @click=${this.#handleOpen}>
              ${folder2Open()}
            </sm-icon-button>
            <sm-icon-button label="Reload tile" @click=${this.#handleReload}>
              ${arrowClockwise()}
            </sm-icon-button>
            <hr>
            <sm-icon-button label="Full screen" @click=${this.#handleFullscreen}>
              ${fullscreen()}
            </sm-icon-button>
          </sm-toolbar>`
      }
      ${
        (!tabs.length || activeIndex < 0)
        ? html`<div class="empty">Open a .tile file to get started</div>`
        : html`
          <sm-tabbed-pane closable ?fullscreen=${isFullscreen} @sm-activate-tab=${this.#handleActivateTab} @sm-close-tab=${this.#handleCloseTab}>
          ${tabs.map((tab, idx) => html`
              <sm-tab-panel label=${tab.masl.name} ?active=${idx === activeIndex}>
                ${(() => {
                  const iconSrc = tab.masl.icons?.[0]?.src;
                  const iconUrl = iconSrc ? new URL(iconSrc, tab.url).href : nothing;
                  return iconSrc ? html`<img src=${iconUrl} alt="icon" slot="icon">` : nothing;
                })()}
                <tnt-model-header></tnt-model-header>
                <iframe src=${tab.url}></iframe>
              </sm-tab-panel>
          `)}
          </sm-tabbed-pane>
        `
      }
    `;
  }
}

customElements.define('tile-app', TileApp);


import { LitElement, html, css, nothing } from 'lit';
import { classMap } from 'lit/directives/class-map.js';
import { SignalWatcher } from '@lit-labs/signals';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import '@supramundane/ui/icon-button';
import '@supramundane/ui/tab-panel';
import '@supramundane/ui/tabbed-pane';
import '@supramundane/ui/toolbar';
import '@supramundane/ui/tokens/light';
import { folder2Open, fullscreen, arrowClockwise, layoutSidebar, layoutSidebarInset } from '@supramundane/ui/icons';
import '../css/arepo.css';
import {
  addTab,
  appStore,
  openTileDialog,
  setFullscreen,
  activateTab,
  closeTab,
  closeActiveTab,
  updateModels,
  openSidebar,
  closeSidebar,
  setUIState,
} from './state.js';
import './el/model-header.js';
import './el/sidebar.js';


// ── Root app shell ────────────────────────────────────────────────────────────
class TileApp extends SignalWatcher (LitElement) {
  static styles = css`
    :host {
      display: flex;
      flex-direction: column;
      height: 100vh;
      overflow: hidden;
    }
    .body {
      flex-grow: 1;
    }
    .empty {
      flex: 1;
      display: flex;
      align-items: center;
      justify-content: center;
      color: var(--sm-color-neutral-500);
      font-size: var(--sm-font-size-medium);
      height: 100%;
    }
    sm-toolbar {
      border-bottom: 1px solid var(--sm-panel-border-color);
      padding-left: 0;
      transition: var(--sm-transition-medium) padding-left;
    }
    sm-toolbar.sidebar-open {
      padding-left: var(--tnt-sidebar-width);
    }
    .body {
      margin-left: 0;
      transition: var(--sm-transition-medium) margin-left;
    }
    .body.sidebar-open {
      margin-left: var(--tnt-sidebar-width);
    }
    tnt-sidebar {
      position: fixed;
      width: var(--tnt-sidebar-width);
      left: calc(-1 * var(--tnt-sidebar-width));
      top: 46px;
      bottom: 0;
      transition: var(--sm-transition-medium) left;
    }
    tnt-sidebar.sidebar-open {
      left: 0;
    }
    sm-tabbed-pane {
      height: 100%;
    }
    sm-tabbed-pane::part(nav) {
      border-top: none;
    }
    iframe {
      flex-grow: 1;
      width: 100%;
      border: none;
    }
    sm-tab-panel[active]::part(base) {
      display: flex;
      flex-direction: column;
      overflow: hidden;
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
    invoke('get_all_ui_state').then((uiState) => {
      console.warn(`ui state`, uiState);
      setUIState(uiState);
    });
    listen('models:changed', (ev) => {
      updateModels(ev.payload);
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
  }
  #handleFullscreen (ev) {
    invoke('set_fullscreen', { fullscreen: true });
  }
  #handleReload () {
    const ifr = this.shadowRoot.querySelector('sm-tab-panel[active] iframe');
    if (!ifr) return;
    ifr.src = ifr.src;
  }

  // XXX
  // - list models with icon and description if available (also no items option)
  // - on hover, show "new" that triggers the right new thing
  //  - option to remove (with confirm - move to trash if that's a thing we can do)
  // - in model-header:
  //  - button to save model (if not already added)
  render () {
    const { fullscreen: isFullscreen } = appStore.get();
    const { tabs, activeIndex, uiState } = appStore.get();
    const { sidebarOpen } = uiState || {};
    return html`
      ${
        isFullscreen
        ? nothing
        : html`<sm-toolbar variant="flat" class=${classMap({ 'sidebar-open': sidebarOpen })}>
            <sm-icon-button label=${sidebarOpen ? "Close side panel" : "Open side panel"} @click=${sidebarOpen ? closeSidebar : openSidebar}>
              ${(sidebarOpen ? layoutSidebarInset : layoutSidebar)()}
            </sm-icon-button>
            <hr>
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
      <tnt-sidebar class=${classMap({ 'sidebar-open': sidebarOpen })}></tnt-sidebar>
      <div class=${classMap({ body: true, 'sidebar-open': sidebarOpen })}>
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
      </div>
    `;
  }
}

customElements.define('tile-app', TileApp);

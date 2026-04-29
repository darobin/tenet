
import { LitElement, html, css } from 'lit';
import { SignalWatcher } from '@lit-labs/signals';
import { appStore } from '../state.js';

export class TileTab extends SignalWatcher(LitElement) {
  static styles = css`
    :host {
      display: flex;
      flex-direction: column;
      flex: 1;
      overflow: hidden;
    }
    .tabs-host {
      display: contents;
    }
    iframe {
      flex: 1;
      border: none;
      width: 100%;
      height: 100%;
      display: block;
    }
    .empty {
      flex: 1;
      display: flex;
      align-items: center;
      justify-content: center;
      color: #555;
      font-size: 15px;
    }
  `;

  render() {
    const { tabs, activeIndex } = appStore.get();
    if (!tabs.length || activeIndex < 0) {
      return html`<div class="empty">Open a .tile file to get started</div>`;
    }
    // Render all tabs but show only the active one. This keeps iframes alive
    // when switching tabs so their content does not reload.
    return html`
      ${tabs.map((tab, i) => html`
        <iframe
          style="display: ${i === activeIndex ? 'block' : 'none'}"
          src=${`tile://${tab.authority}/`}
          title=${tab.masl.name}
        ></iframe>
      `)}
    `;
  }
}

customElements.define('tile-content', TileTab);

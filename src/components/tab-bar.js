
import { LitElement, html, css, nothing } from 'lit';
import { SignalWatcher } from '@lit-labs/signals';
import { appStore, activateTab, closeTab, openTileDialog } from '../state.js';

export class TabBar extends SignalWatcher(LitElement) {
  static styles = css`
    :host {
      display: flex;
      align-items: center;
      background: #2d2d2d;
      height: 38px;
      overflow-x: auto;
      overflow-y: hidden;
      scrollbar-width: thin;
      flex-shrink: 0;
    }
    .open-btn {
      flex-shrink: 0;
      margin: 4px 6px;
      padding: 2px 10px;
      background: #3a3a3a;
      border: 1px solid #555;
      border-radius: 4px;
      color: #ccc;
      cursor: pointer;
      font-size: 13px;
      white-space: nowrap;
    }
    .open-btn:hover { background: #4a4a4a; }
    .tab {
      display: flex;
      align-items: center;
      gap: 5px;
      height: 100%;
      padding: 0 10px;
      cursor: pointer;
      border-right: 1px solid #222;
      color: #999;
      font-size: 13px;
      white-space: nowrap;
      max-width: 200px;
      min-width: 80px;
      position: relative;
      flex-shrink: 0;
    }
    .tab:hover { background: #3a3a3a; color: #ccc; }
    .tab.active { background: #1e1e1e; color: #fff; }
    .tab img {
      width: 16px;
      height: 16px;
      object-fit: contain;
      flex-shrink: 0;
    }
    .tab-label {
      overflow: hidden;
      text-overflow: ellipsis;
      flex: 1;
    }
    .close {
      flex-shrink: 0;
      width: 16px;
      height: 16px;
      border-radius: 50%;
      border: none;
      background: transparent;
      color: inherit;
      font-size: 14px;
      line-height: 1;
      cursor: pointer;
      display: flex;
      align-items: center;
      justify-content: center;
      padding: 0;
    }
    .close:hover { background: #555; }
  `;

  render() {
    const { tabs, activeIndex } = appStore.get();
    return html`
      <button class="open-btn" @click=${this._openFile}>+ Open</button>
      ${tabs.map((tab, i) => this._renderTab(tab, i, activeIndex))}
    `;
  }

  _renderTab(tab, index, activeIndex) {
    const iconSrc = tab.masl.icons?.[0]?.src;
    const iconUrl = iconSrc ? `tile://${tab.authority}${iconSrc}` : nothing;
    return html`
      <div
        class="tab ${index === activeIndex ? 'active' : ''}"
        @click=${() => activateTab(index)}
      >
        ${iconSrc ? html`<img src=${iconUrl} alt="" />` : nothing}
        <span class="tab-label">${tab.masl.name}</span>
        <button
          class="close"
          title="Close"
          @click=${(e) => { e.stopPropagation(); closeTab(index); }}
        >×</button>
      </div>
    `;
  }

  _openFile() {
    return openTileDialog();
  }
}

customElements.define('tile-tab-bar', TabBar);

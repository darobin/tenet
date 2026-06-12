
import { LitElement, html, css, nothing } from 'lit';
import { SignalWatcher } from '@lit-labs/signals';
import { setTileName, activeTab } from '../state.js';

class Sidebar extends SignalWatcher (LitElement) {
  static styles = css`
    :host {
      display: flex;
    }
    .sidebar {
      margin: var(--sm-spacing-small);
      padding: var(--sm-spacing-small);
      border: var(--sm-panel-border-width) solid var(--sm-color-accent-200);
      border-radius: var(--sm-border-radius-x-small);
      flex-grow: 1;
    }
    h2 {
      font-family: var(--sm-input-font-family);
      margin: 0 0 var(--sm-spacing-small) 0;
      font-size: var(--sm-font-size-medium);
      font-weight: var(--sm-font-weight-medium);
    }
    .empty {
      color: var(--sm-color-neutral-500);
      font-size: var(--sm-font-size-medium);
    }
  `;

  render() {
    const { models } = appStore.get();
    let modelsContent = nothing;
    if (models?.length) {
      modelsContent = html``;
    }
    else {
      modelsContent = html`<div class="empty">No models saved yet.</div>`;
    }
    return html`
      <div class="sidebar">
        <section>
          <h2>Models</h2>
          ${modelsContent}
        </section>
      </div>
    `;
  }
}

customElements.define('tnt-sidebar', Sidebar);

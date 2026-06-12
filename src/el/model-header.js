
import { LitElement, html, css, nothing } from 'lit';
import { SignalWatcher } from '@lit-labs/signals';
import '@supramundane/ui/input';
import "@supramundane/ui/button";
import "@supramundane/ui/icon";
import { floppy } from '@supramundane/ui/icons';
import { setTileName, activeTab, addModel, appStore } from '../state.js';

class ModelHeader extends SignalWatcher (LitElement) {
  static styles = css`
    :host {
      display: block;
    }
    form {
      flex-grow: 1;
    }
    .model-header {
      display: flex;
      gap: var(--sm-spacing-small);
      padding: var(--sm-spacing-small);
      background: var(--sm-color-accent-50);
      border-bottom: 1px solid var(--sm-panel-border-color);
    }
    #name::part(input) {
      font-weight: 500;
      field-sizing: content;
      min-width: 50px;
    }
    #name::part(base) {
      background-color: transparent;
      border-color: transparent;
      transition: var(--sm-transition-medium) all;
    }
    #name::part(base):hover, #name::part(base focused) {
      background-color: var(--sm-input-background-color);
      border-color: var(--sm-input-border-color);
    }
    .actions {
      align-content: center;
    }
  `;

  #handleSubmit (ev) {
    ev.preventDefault();
    const authority = activeTab()?.authority;
    setTileName(authority, ev.target.querySelector('#name').value);
  }
  #handleBlur (ev) {
    const authority = activeTab()?.authority;
    setTileName(authority, ev.target.value);
  }
  #handleSaveModel () {
    const authority = activeTab()?.authority;
    addModel(authority);
  }
  // For now it's only the name, we can add description and banner later.
  render () {
    const masl = activeTab()?.masl;
    if (!masl?.model) return nothing;
    const { models } = appStore.get();
    const savedModel = models?.find(m => m.id === masl.model.id);
    return html`
      <div class="model-header">
        <form @submit=${this.#handleSubmit}>
          <sm-input @blur=${this.#handleBlur} value=${masl.name || masl.model?.name} id="name" placeholder="Document title"></sm-input>
        </form>
        <div class="actions">
          ${
            savedModel
            ? html`<sm-button disabled outline>${floppy({ slot: 'prefix' })}Model saved</sm-button>`
            : html`<sm-button @click=${this.#handleSaveModel}>${floppy({ slot: 'prefix' })}Save Model</sm-button>`
          }
        </div>
      </div>
    `;
  }
}

customElements.define('tnt-model-header', ModelHeader);

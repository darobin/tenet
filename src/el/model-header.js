
import { LitElement, html, css, nothing } from 'lit';
import { SignalWatcher } from '@lit-labs/signals';
import '@supramundane/ui/input';
import { setTileName, activeTab } from '../state.js';

class ModelHeader extends SignalWatcher (LitElement) {
  static styles = css`
    :host {
      display: block;
    }
    .model-header {
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
  // For now it's only the name, we can add description and banner later.
  render () {
    const masl = activeTab()?.masl;
    if (!masl?.model) return nothing;
    return html`
      <div class="model-header">
        <form @submit=${this.#handleSubmit}>
          <sm-input @blur=${this.#handleBlur} value=${masl.name || masl.model?.name} id="name"></sm-input>
        </form>
      </div>
    `;
  }
}

customElements.define('tnt-model-header', ModelHeader);

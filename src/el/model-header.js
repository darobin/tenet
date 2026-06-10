
import { LitElement, html, css, nothing } from 'lit';
import { SignalWatcher } from '@lit-labs/signals';
import '@supramundane/ui/input';
import { setTileName, activeTab } from '../state.js';


class ModelHeader extends SignalWatcher (LitElement) {
  static styles = css`
    :host {
      display: block;
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
    console.warn(`masl`, masl);
    if (!masl?.model) return nothing;
    return html`
      <form class="model-header" @submit=${this.#handleSubmit}>
        <sm-input @blur=${this.#handleBlur} value=${masl.name || masl.model?.name} id="name"></sm-input>
      </form>
    `;
  }
}

customElements.define('tnt-model-header', ModelHeader);

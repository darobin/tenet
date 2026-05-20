
import { LitElement, html, css } from 'lit';
import { SignalWatcher } from '@lit-labs/signals';
import { iconOpen } from '@supramundane/ui/icons';
import { openTileDialog } from '../state.js';
import { iconButton } from '../style.js';


export class Toolbar extends SignalWatcher (LitElement) {
  static styles = [css`
    :host {
      display: flex;
      overflow: hidden;
      padding: 4px;
      min-height: 32px;
    }
    sm-icon-button {
      margin: 0 8px;
    }
    `,
    iconButton,
  ];

  handleOpen (ev) {
    return openTileDialog();
  }
  render () {
    return html`
      <sm-icon-button label="Open tile" @click=${this.handleOpen}>
        ${iconOpen()}
      </sm-icon-button>
    `;
  }
}

customElements.define('tile-toolbar', Toolbar);

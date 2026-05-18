
import { LitElement, html, css } from 'lit';
import { SignalWatcher } from '@lit-labs/signals';
import { openTileDialog } from '../state.js';
import { iconButton } from '../style.js';

import { iconOpen } from '@supramundane/ui/icons';

export class Toolbar extends SignalWatcher (LitElement) {
  static styles = [css`
    :host {
      display: flex;
      overflow: hidden;
      padding: 4px;
    }
    ag-icon-button {
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
        ${iconOpen({ slot: 'icon' })}
      </sm-icon-button>
    `;
  }
}

customElements.define('tile-toolbar', Toolbar);

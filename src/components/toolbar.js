
import { LitElement, html, css } from 'lit';
import { SignalWatcher } from '@lit-labs/signals';
import { openTileDialog } from '../state.js';
import { iconButton } from '../style.js';

import './ag/IconButton/core/IconButton.js';
import openIcon from '../icons/folder-open.svg?lit';

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
      <ag-icon-button label="Open tile" @icon-button-click=${this.handleOpen}>
        ${openIcon}
      </ag-icon-button>
    `;
  }
}

customElements.define('tile-toolbar', Toolbar);

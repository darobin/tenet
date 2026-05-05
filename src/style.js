
import { LitElement, html, css } from 'lit';

export const iconButton = css`
  ag-icon-button svg {
    width: 100%;
    height: 100%;
    max-width: 100%;
    max-height: 100%;
  }
  ag-icon-button::part(ag-icon-has-slotted) {
    min-width: 20px;
    min-height: 20px;
  }
`;

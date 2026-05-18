
const div = document.querySelector('#logo');
const colours = window.randomColor({ count: 25, format: 'hsl' });
const { svg } = htmlHelpers(document);
const root = svg('svg', { width: '500px', height: '500px', viewBox: '0 0 500 500' }, [], div);

colours.forEach((fill, idx) => {
  const x = (idx % 5) * 100;
  const y = Math.floor(idx / 5) * 100;
  console.warn(x, y);
  svg('rect', { x, y, width: 100, height: 100, fill }, [], root);
});

Array.from({ length: 6 }, (_, index) => index).forEach(idx => {
  const x = idx * 100;
  const sw = (idx === 0 || idx === 5) ? 12 : 6;
  svg('line', { x1: x, x2: x, y1: 0, y2: 500, 'stroke-width': sw, stroke: '#fff' }, [], root);
  svg('line', { x1: 0, x2: 500, y1: x, y2: x, 'stroke-width': sw, stroke: '#fff' }, [], root);
});

function htmlHelpers (doc) {
  function el(n, attrs, kids, p) {
    let e;
    if (Array.isArray(n)) e = doc.createElementNS(n[0], n[1]);
    else e = doc.createElement(n);
    Object.entries(attrs || {}).forEach(([k, v]) => {
      if (v == null) return;
      e.setAttribute(k, v);
    });
    (kids || []).forEach(appendByType(e));
    if (p) p.append(e);
    return e;
  }
  function svg (n, attrs, kids, p) {
    return el(['http://www.w3.org/2000/svg', n], attrs, kids, p);
  }

  function df (...nodes) {
    const df = doc.createDocumentFragment();
    (nodes || []).forEach(appendByType(df));
    return df;
  }

  function appendByType (parent) {
    return (n) => {
      if (typeof n === 'string') parent.append(txt(n));
      else parent.append(n);
    }
  }

  function txt (str) {
    return doc.createTextNode(str);
  }

  return ({
    el,
    svg,
    df,
    txt,
  });
}

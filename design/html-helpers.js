
export default function htmlHelpers (doc) {
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

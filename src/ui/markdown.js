// The Markdown Claude answers in, as HTML for the panel: paragraphs, headings, lists, tables, code, bold.
// Works in the browser (window.markdown) and in Node (require). Text is escaped first, so only these tags come out.
(function (root) {
  const esc = s => String(s).replace(/[&<>"']/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[c]);
  // `code` stays literal; links show their text (the panel opens no pages).
  const inline = s => s.split(/(`[^`]+`)/).map((x, i) => (i % 2 ? `<code>${esc(x.slice(1, -1))}</code>`
    : esc(x).replace(/\[([^\]]+)\]\([^)\s]+\)/g, '$1').replace(/\*\*(.+?)\*\*/g, '<strong>$1</strong>'))).join('');
  const cells = l => l.trim().replace(/^\||\|$/g, '').split('|').map(c => inline(c.trim()));

  function markdown(text) {
    const lines = String(text ?? '').replace(/\r\n?/g, '\n').split('\n'), out = [];
    let para = [], list = null; // list: { tag, start, items }
    const flush = () => {
      if (para.length) out.push(`<p>${para.map(inline).join('<br>')}</p>`);
      if (list) out.push(`<${list.tag}${list.start > 1 ? ` start="${list.start}"` : ''}>${list.items.map(x => `<li>${inline(x)}</li>`).join('')}</${list.tag}>`);
      para = []; list = null;
    };
    for (let i = 0; i < lines.length; i++) {
      const l = lines[i];
      let m;
      if (/^\s*```/.test(l)) { // up to the closing fence, or the end (a long report is cut)
        flush();
        const code = [];
        while (++i < lines.length && !/^\s*```/.test(lines[i])) code.push(lines[i]);
        out.push(`<pre><code>${esc(code.join('\n'))}</code></pre>`);
      } else if (/^\s*\|/.test(l) && /^\s*\|?\s*:?-{3,}/.test(lines[i + 1] || '')) { // header row, then |---|
        flush();
        const head = cells(l), rows = [];
        for (i++; /^\s*\|/.test(lines[i + 1] || '');) rows.push(cells(lines[++i]));
        out.push(`<table><thead><tr>${head.map(c => `<th>${c}</th>`).join('')}</tr></thead>`
          + `<tbody>${rows.map(r => `<tr>${r.map(c => `<td>${c}</td>`).join('')}</tr>`).join('')}</tbody></table>`);
      } else if ((m = /^#{1,6}\s+(.*)$/.exec(l))) { flush(); out.push(`<h4>${inline(m[1])}</h4>`); }
      else if (/^\s*([-*_])(\s*\1){2,}\s*$/.test(l)) { flush(); out.push('<hr>'); }
      else if ((m = /^\s*([-*+]|\d+[.)])\s+(.*)$/.exec(l))) {
        const tag = /\d/.test(m[1]) ? 'ol' : 'ul';
        if (para.length || list?.tag !== tag) flush();
        list ??= { tag, start: parseInt(m[1], 10) || 1, items: [] };
        list.items.push(m[2]);
      } else if (!l.trim()) flush();
      else if (list && /^\s/.test(l)) list.items[list.items.length - 1] += ' ' + l.trim(); // a list item's next line
      else { if (list) flush(); para.push(l.trim()); }
    }
    flush();
    return out.join('');
  }

  if (typeof module === 'object' && module.exports) module.exports = markdown; else root.markdown = markdown;
})(this);

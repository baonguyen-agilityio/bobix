// The Markdown Claude answers in, and prompts are written in, as HTML for the panel: paragraphs, headings, nested
// lists, tables, code, quotes, bold and italic.
// Works in the browser (window.markdown) and in Node (require). Text is escaped first, so only these tags come out.
(function (root) {
  const esc = s => String(s).replace(/[&<>"']/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[c]);
  // `code` stays literal; links and images show their text (the panel opens no pages, so no href ever comes out).
  // Italic needs a letter-free edge, so `2 * 3 * 4`, snake_case and __init__ stay as written. Link text and italic never
  // hold their own marker, so a 32k line full of "[" or "*" is read once, not once per marker.
  const inline = s => s.split(/(`[^`]+`)/).map((x, i) => (i % 2 ? `<code>${esc(x.slice(1, -1))}</code>`
    : esc(x).replace(/!?\[([^[\]]+)\]\((?:[^()\s]|\([^()\s]*\))+\)/g, '$1').replace(/\*\*(.+?)\*\*/g, '<strong>$1</strong>')
      .replace(/(?<![\p{L}\p{N}_*])(\*(?=[^\s*])[^*]+?(?<=[^\s*])\*|_(?=[^\s_])[^_]+?(?<=[^\s_])_)(?![\p{L}\p{N}_*])/gu, m => `<em>${m.slice(1, -1)}</em>`))).join('');
  // Pipes inside `code` or escaped as \| (GFM) are part of the cell, not a border.
  const cells = l => l.trim().replace(/^\||(?<!\\)\|$/g, '').split(/(?<!\\)\|(?=(?:[^`]*`[^`]*`)*[^`]*$)/).map(c => inline(c.trim().replace(/\\\|/g, '|')));

  function markdown(text) {
    const lines = String(text ?? '').replace(/\r\n?/g, '\n').split('\n'), out = [];
    let para = [], lists = []; // lists: the open ones, outermost first: { tag, start, ind, items: [{ text, kids }] }
    const close = () => {
      const x = lists.pop(), html = `<${x.tag}${x.start > 1 ? ` start="${x.start}"` : ''}>${x.items.map(it => `<li>${inline(it.text)}${it.kids}</li>`).join('')}</${x.tag}>`;
      if (lists.length) lists.at(-1).items.at(-1).kids += html; else out.push(html);
    };
    const flush = () => {
      if (para.length) out.push(`<p>${para.map(inline).join('<br>')}</p>`);
      para = [];
      while (lists.length) close();
    };
    for (let i = 0; i < lines.length; i++) {
      const l = lines[i];
      let m;
      if ((m = /^(\s*)(`{3,}|~{3,})/.exec(l))) { // up to the closing fence, or the end (a long text is cut)
        flush();
        const code = [], end = new RegExp(`^\\s*${m[2][0]}{${m[2].length},}\\s*$`), indent = new RegExp(`^ {0,${m[1].length}}`);
        while (++i < lines.length && !end.test(lines[i])) code.push(lines[i].replace(indent, ''));
        out.push(`<pre><code>${esc(code.join('\n'))}</code></pre>`);
      } else if (/^\s*\|/.test(l) && /^\s*\|?\s*:?-+:?\s*\|/.test(lines[i + 1] || '')) { // header row, then |---|
        flush();
        const head = cells(l), rows = [];
        for (i++; /^\s*\|/.test(lines[i + 1] || '');) rows.push(cells(lines[++i]));
        out.push(`<table><thead><tr>${head.map(c => `<th>${c}</th>`).join('')}</tr></thead>`
          + `<tbody>${rows.map(r => `<tr>${r.map(c => `<td>${c}</td>`).join('')}</tr>`).join('')}</tbody></table>`);
      } else if ((m = /^#{1,6}\s+(.*?)(\s+#+)?\s*$/.exec(l))) { flush(); out.push(`<h4>${inline(m[1])}</h4>`); }
      else if (/^\s*([-*_])(\s*\1){2,}\s*$/.test(l)) { flush(); out.push('<hr>'); }
      else if (/^\s*>/.test(l)) { // nested quotes come out as one level: no recursion a long ">>>>" line could overflow
        flush();
        const q = [l];
        while (/^\s*>/.test(lines[i + 1] || '')) q.push(lines[++i]);
        out.push(`<blockquote>${markdown(q.map(x => x.replace(/^(\s*>)+\s?/, '')).join('\n'))}</blockquote>`);
      } else if ((m = /^(\s*)([-*+]|\d+[.)])\s+(.*)$/.exec(l))) {
        const ind = m[1].replace(/\t/g, '    ').length, tag = /\d/.test(m[2]) ? 'ol' : 'ul';
        if (para.length) flush();
        while (lists.length > 1 && ind <= lists.at(-2).ind) close(); // back out to this item's own list
        if (lists.length && ind < lists.at(-1).ind + 2 && lists.at(-1).tag !== tag) close(); // "- x" after "1. y": a new list
        if (!lists.length || ind >= lists.at(-1).ind + 2 || lists.at(-1).tag !== tag) lists.push({ tag, ind, start: parseInt(m[2], 10) || 1, items: [] });
        lists.at(-1).items.push({ text: m[3].replace(/^\[([ xX])\]\s+/, (_, c) => (c === ' ' ? '☐ ' : '☑ ')), kids: '' });
      } else if (!l.trim()) { if (!lists.length) flush(); } // a blank line inside a list keeps it open (loose lists, nested blocks)
      else if (lists.length && /^\s/.test(l)) lists.at(-1).items.at(-1).text += ' ' + l.trim(); // a list item's next line
      else { if (lists.length) flush(); para.push(l.trim()); }
    }
    flush();
    return out.join('');
  }

  if (typeof module === 'object' && module.exports) module.exports = markdown; else root.markdown = markdown;
})(this);

// An SVG chosen as a pet picture. It is never served or shown as it is: a hidden page draws it as an image
// (see src/decode.js), which runs no script and loads nothing outside the file, and the bitmap goes through the
// same pixel art as any other picture. Pure (no electron), so it is tested directly.

// Up to the root element: an XML declaration, comments and a DOCTYPE without an internal subset.
const PROLOG = /^\s*(?:<\?xml[^>]*\?>\s*)?(?:(?:<!--[\s\S]*?-->|<!DOCTYPE[^>[]*>)\s*)*/i;
const ROOT = /^<svg(?=[\s>/])([^>]*)>/i;
const HEAD = 64 * 1024; // the root tag must start within this many bytes

function rootTag(buf) {
  const text = buf.toString('utf8', 0, Math.min(buf.length, HEAD)).replace(/^﻿/, '');
  const m = ROOT.exec(text.slice(PROLOG.exec(text)[0].length));
  return m ? m[1] : null;
}

// True for an SVG file, from its bytes. Entity declarations are refused: they can make a small file expand
// into a huge one while it is parsed.
const isSvg = buf => rootTag(buf) !== null && !buf.includes('<!ENTITY');

const PX = { px: 1, pt: 4 / 3, pc: 16, in: 96, cm: 96 / 2.54, mm: 96 / 25.4 };
const length = v => {
  const m = /^\s*(\d*\.?\d+(?:e[-+]?\d+)?)\s*(px|pt|pc|in|cm|mm)?\s*$/i.exec(v || '');
  const n = m ? Number(m[1]) * PX[(m[2] || 'px').toLowerCase()] : NaN;
  return n > 0 && Number.isFinite(n) ? n : null;
};

// The size to draw the picture at: its shape from the viewBox, else from an absolute width and height, else the
// 300×150 a browser gives an SVG with neither; the long side is always `side`, the shape kept within 8:1.
function drawSize(buf, side) {
  const tag = rootTag(buf) || '', attrs = {};
  for (const m of tag.matchAll(/([\w:-]+)\s*=\s*(?:"([^"]*)"|'([^']*)')/g)) attrs[m[1].toLowerCase()] = m[2] ?? m[3];
  const box = (attrs.viewbox || '').trim().split(/[\s,]+/).map(Number);
  let w = 300, h = 150;
  if (box.length === 4 && box[2] > 0 && box[3] > 0 && box.every(Number.isFinite)) [w, h] = [box[2], box[3]];
  else [w, h] = [length(attrs.width) ?? w, length(attrs.height) ?? h];
  const r = Math.min(8, Math.max(1 / 8, w / h));
  return r >= 1 ? { width: side, height: Math.max(1, Math.round(side / r)) } : { width: Math.max(1, Math.round(side * r)), height: side };
}

module.exports = { isSvg, drawSize };

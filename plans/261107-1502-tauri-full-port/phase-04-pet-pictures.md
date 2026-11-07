# Phase 4: pet pictures

Back to [plan](plan.md). Still Electron for users: everything.

## Context
- `main.js` pet picture section (`choosePetImage`, `startImport`, `runImport`, `savePetImage`, `resetPetImage`, `petImageActs`, `decodeWebp`, `importInput`), `core/pixelate.js`, `core/pixelate-worker.js`, `core/gif/*`, `core/petstore.js`, `server.js` `/pet-image/`.

## Requirements
- Same panel calls and replies: `petImage(id, 'choose'|'edit'|'preview'|'save'|'cancel'|'reset', opts)` → `{ ok, preview, opts, opaque, animated, frames, ms }` or `{ ok:false, reason: bad|big|gone|save }`.
- Rust: file dialog (`tauri-plugin-dialog`, same filters and title), 2 MB cap, `kindOf` by bytes, original handed to the panel; `petstore.rs` (hash names, `originals/`, write tmp + rename, `release` of unused files, `readOriginal`); `/pet-image/<24 hex>.(png|gif)` with the token; save checks the result is a PNG/GIF the size of the art and ≤ 2 MB before writing; settings saved before old files are released.
- Panel side (`src/ui/pet-import.js`, loaded by the bridge, not by `panel.html`): decode PNG/JPG/WebP with `createImageBitmap` + canvas at most 1024 px; GIF as bytes; `pixelate.js` in a Web Worker with the same 20 s stuck-worker timeout; one import at a time.
- `pixelate.js` loses its Node-only parts: `zlib.deflateSync` → `CompressionStream('deflate')` (or stored blocks if missing), `zlib.crc32` → a 10-line CRC table, `Buffer` → `Uint8Array`. It keeps working in `node --test`.

## Files
- Create: `src/ui/pet-import.js`, `src/ui/pixelate-worker.js` (from `core/`), `src-tauri/src/petstore.rs`.
- Move: `src/core/pixelate.js`, `src/core/gif/*` → `src/ui/` (served to the panel; node tests import from there).
- Edit: `server.rs`, `bridge.js`, `settings.rs` (`normalizeOpts` checks on stored opts), tests.

## Validation
- Node tests for pixelate unchanged and green; `cargo test` for petstore (same cases as today).
- macOS by eye: choose PNG, JPG, GIF (animated), WebP; adjust options; save; pet shows it; Edit picture reopens from the original; Default pet removes files no one uses; a 3 MB file says too big; a renamed `.txt` says bad.

## Risks / rollback
- WebKit's `CompressionStream` / `OffscreenCanvas` availability on macOS 11–13: decode on the panel's main thread with a normal canvas, and fall back to stored (uncompressed) deflate for 64 px art.
- Rollback: revert the PR.

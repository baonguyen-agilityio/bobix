# Runs src/ui/pet-import.js's decode in WebKitGTK (the engine Tauri uses on Linux) on the SVG fixtures and reports
# pixels plus every request that reached a "links" server the fixtures point at. Also a control: the same script.svg
# opened as a document (not an image) must reach that server, proving the counter works.
# Usage: python3 -I webkit_svg.py <worktree>
import sys, os, json, threading, http.server, socketserver
import gi
gi.require_version('Gtk', '3.0'); gi.require_version('WebKit2', '4.1')
from gi.repository import Gtk, WebKit2, GLib

root = sys.argv[1]
ui = os.path.join(root, 'src/ui'); fx = os.path.join(root, 'test/fixtures')
hits = []

class Links(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        hits.append(self.path); self.send_response(200); self.end_headers()
    def log_message(self, *a): pass

class Pages(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        if self.path == '/':
            body, ty = b'<!doctype html><title>t</title>', 'text/html'
        elif self.path == '/ui/pet-import.js':
            body, ty = open(os.path.join(ui, 'pet-import.js'), 'rb').read(), 'text/javascript'
        elif self.path == '/doc/script.svg':
            body, ty = fixture('script.svg').encode(), 'image/svg+xml'
        else:
            self.send_response(404); self.end_headers(); return
        self.send_response(200); self.send_header('content-type', ty); self.end_headers(); self.wfile.write(body)
    def log_message(self, *a): pass

def serve(h):
    s = socketserver.ThreadingTCPServer(('127.0.0.1', 0), h); s.daemon_threads = True
    threading.Thread(target=s.serve_forever, daemon=True).start(); return s

links = serve(Links); pages = serve(Pages)
origin = 'http://127.0.0.1:%d' % links.server_address[1]
base = 'http://127.0.0.1:%d' % pages.server_address[1]
def fixture(n): return open(os.path.join(fx, n), encoding='utf8').read().replace('ORIGIN', origin)

JS = """
await new Promise((ok, no) => document.head.append(Object.assign(document.createElement('script'), { src: '/ui/pet-import.js', onload: ok, onerror: no })));
const pi = window.__bobixPetImport(() => Promise.resolve(null)), bytes = s => new TextEncoder().encode(s);
const px = (d, x, y) => [...d.data.subarray((y * d.width + x) * 4, (y * d.width + x) * 4 + 4)];
const draw = (s, size) => pi.decode(bytes(s), 'svg', size).then(d => ({ width: d.width, height: d.height, d }), e => ({ error: String(e) }));
const v = await draw(F.viewbox, { width: 64, height: 32 });
const s = await draw(F.script, { width: 64, height: 64 });
const f = await draw(F.foreign, { width: 64, height: 64 });
const broken = await pi.startImport('/p', bytes('<svg xmlns="http://www.w3.org/2000/svg"><rect'), 'svg', undefined, { width: 64, height: 64 });
await new Promise(ok => setTimeout(ok, 500));
return JSON.stringify({ ua: navigator.userAgent, viewbox: v.d ? { width: v.width, height: v.height, left: px(v.d, 2, 16), right: px(v.d, 61, 16) } : v,
  script: s.d ? { width: s.width, height: s.height, middle: px(s.d, 32, 32) } : s, foreign: f.d ? { middle: px(f.d, 32, 32) } : f.error, broken });
"""

out = {}
win = Gtk.OffscreenWindow(); view = WebKit2.WebView(); win.add(view); win.set_default_size(200, 200); win.show_all()
stage = ['decode']

def finish():
    out['hitsAfterDecode'] = out.get('hitsAfterDecode', list(hits))
    Gtk.main_quit()

def on_js(v, res):
    try: out['decode'] = json.loads(v.call_async_javascript_function_finish(res).to_string())
    except Exception as e: out['decode'] = 'error: %s' % e
    out['hitsAfterDecode'] = list(hits)
    stage[0] = 'control'
    view.load_uri(base + '/doc/script.svg')  # control: the SVG as a document does load its links

def on_load(v, ev):
    if ev != WebKit2.LoadEvent.FINISHED: return
    if stage[0] == 'decode':
        args = GLib.Variant('a{sv}', {'F': GLib.Variant('a{sv}', {k: GLib.Variant('s', fixture(k + '.svg')) for k in ('viewbox', 'script', 'foreign')})})
        v.call_async_javascript_function(JS, -1, args, None, None, None, on_js)
    elif stage[0] == 'control':
        stage[0] = 'done'
        def end():
            out['controlHits'] = [h for h in hits if h not in out['hitsAfterDecode']]
            Gtk.main_quit(); return False
        GLib.timeout_add(1500, end)

view.connect('load-changed', on_load)
GLib.timeout_add(30000, lambda: (out.setdefault('timeout', True), Gtk.main_quit()))
view.load_uri(base + '/')
Gtk.main()
print(json.dumps(out, indent=1))

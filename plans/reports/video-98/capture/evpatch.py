# Evidence-only (never committed in src): panel size, opening tab and a page script from env vars; a page script for the pets window too.
import sys
root = sys.argv[1]
p = root + '/src-tauri/src/panel.rs'; s = open(p).read()
a = '''.title("Bobix").inner_size(1180.0, 780.0)'''
assert s.count(a) == 1
s = s.replace(a, '''.title("Bobix").inner_size(std::env::var("EVIDENCE_PANEL_W").ok().and_then(|v| v.parse().ok()).unwrap_or(1180.0), std::env::var("EVIDENCE_PANEL_H").ok().and_then(|v| v.parse().ok()).unwrap_or(780.0))''')
a = '''    let (app2, b) = (app.clone(), b.clone());'''
assert s.count(a) == 1
s = s.replace(a, a + '''\n    let tab = tab.or_else(|| std::env::var("EVIDENCE_TAB").ok());''')
a = '''            .initialization_script(crate::BRIDGE)'''
assert s.count(a) == 1
s = s.replace(a, a + '''\n            .initialization_script(std::fs::read_to_string(std::env::var("EVIDENCE_PANEL_JS").unwrap_or_default()).unwrap_or_default())''')
a = '''            .initialization_script(crate::BRIDGE)'''
s = s.replace(a, a + '''\n            .theme(std::env::var_os("EVIDENCE_LIGHT").map(|_| Theme::Light))''')
open(p, 'w').write(s)
p = root + '/src-tauri/src/overlay.rs'; s = open(p).read()
a = '''        .initialization_script(crate::BRIDGE)'''
assert s.count(a) == 1
s = s.replace(a, a + '''\n        .initialization_script(std::fs::read_to_string(std::env::var("EVIDENCE_PETS_JS").unwrap_or_default()).unwrap_or_default())\n        .theme(std::env::var_os("EVIDENCE_LIGHT").map(|_| tauri::Theme::Light))''')
open(p, 'w').write(s)
print('patched')

# Evidence-only (never committed): the panel window's opening size and theme from EVIDENCE_PANEL_W / _H / EVIDENCE_DARK.
import sys
p=sys.argv[1]+'/src-tauri/src/panel.rs'; s=open(p).read()
a='''.title("Bobix").inner_size(1180.0, 780.0)'''
assert s.count(a)==1
s=s.replace(a,'''.title("Bobix").inner_size(std::env::var("EVIDENCE_PANEL_W").ok().and_then(|v| v.parse().ok()).unwrap_or(1180.0), std::env::var("EVIDENCE_PANEL_H").ok().and_then(|v| v.parse().ok()).unwrap_or(780.0))
            .theme(std::env::var_os("EVIDENCE_DARK").map(|_| Theme::Dark))''')
a='''    let (app2, b) = (app.clone(), b.clone());'''
assert s.count(a)==1
s=s.replace(a, a+'''\n    let tab = tab.or_else(|| std::env::var("EVIDENCE_TAB").ok());''')
a='''            .initialization_script(crate::BRIDGE)'''
assert s.count(a)==1
s=s.replace(a, a+'''\n            .initialization_script(std::env::var("EVIDENCE_JS").unwrap_or_default())''')
open(p,'w').write(s)

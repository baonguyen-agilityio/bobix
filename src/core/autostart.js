// Start-at-login on macOS, Windows and Linux, for both the packaged app and `npm start` from a clone.
const fs = require('fs');
const os = require('os');
const path = require('path');

const LABEL = 'dev.bobix';
const linuxFile = () => path.join(os.homedir(), '.config', 'autostart', 'bobix.desktop');
const macDevFile = () => path.join(os.homedir(), 'Library', 'LaunchAgents', `${LABEL}.plist`);
const xml = s => String(s).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');

// What to run at login: the packaged executable, else electron + the app folder.
function command(app) {
  return app.isPackaged ? [process.execPath] : [process.execPath, app.getAppPath()];
}

function set(app, on) {
  const argv = command(app);
  if (process.platform === 'linux') {
    const f = linuxFile();
    if (!on) return fs.rmSync(f, { force: true });
    fs.mkdirSync(path.dirname(f), { recursive: true });
    fs.writeFileSync(f, `[Desktop Entry]\nType=Application\nName=Bobix\nExec=${argv.map(a => `"${a.replace(/(["\\$`])/g, '\\$1')}"`).join(' ')}\nX-GNOME-Autostart-enabled=true\n`);
  } else if (process.platform === 'darwin' && !app.isPackaged) {
    // Login items can only point at an .app bundle; a LaunchAgent runs any command.
    const f = macDevFile();
    if (!on) return fs.rmSync(f, { force: true });
    fs.mkdirSync(path.dirname(f), { recursive: true });
    fs.writeFileSync(f, `<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>Label</key><string>${LABEL}</string>
<key>ProgramArguments</key><array>${argv.map(a => `<string>${xml(a)}</string>`).join('')}</array>
<key>RunAtLoad</key><true/>
</dict></plist>
`);
  } else {
    app.setLoginItemSettings(app.isPackaged ? { openAtLogin: on } : { openAtLogin: on, path: argv[0], args: argv.slice(1) });
  }
}

function get(app) {
  if (process.platform === 'linux') return fs.existsSync(linuxFile());
  if (process.platform === 'darwin' && !app.isPackaged) return fs.existsSync(macDevFile());
  const argv = command(app);
  return app.getLoginItemSettings(app.isPackaged ? {} : { path: argv[0], args: argv.slice(1) }).openAtLogin;
}

module.exports = { set, get };

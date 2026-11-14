# Installing Bobix

This guide is for anyone you share Bobix with. It takes about two minutes.

**You need:** Claude Code (CLI, the Claude desktop app, or an IDE extension) on a recent macOS, Windows 10/11, or Ubuntu / Debian (X11 or Wayland).
Node.js is recommended but not required (see [Without Node.js](#without-nodejs)).

## 1. Get the installer

Download the installer for your computer (the links always give the newest version), or use the file someone sent you. Install from these files: the repository's source is for contributors.

| Your computer | Download |
|---|---|
| Mac with Apple chip (M1 and later) | [Bobix-mac-arm64.dmg](https://github.com/baonguyen-agilityio/bobix/releases/latest/download/Bobix-mac-arm64.dmg) |
| Mac with Intel chip | [Bobix-mac-x64.dmg](https://github.com/baonguyen-agilityio/bobix/releases/latest/download/Bobix-mac-x64.dmg) |
| Windows 10 / 11 (64-bit) | [Bobix-windows-x64.exe](https://github.com/baonguyen-agilityio/bobix/releases/latest/download/Bobix-windows-x64.exe) |
| Ubuntu / Debian (amd64) | [bobix-linux-amd64.deb](https://github.com/baonguyen-agilityio/bobix/releases/latest/download/bobix-linux-amd64.deb) |
| Ubuntu / Debian (ARM64) | [bobix-linux-arm64.deb](https://github.com/baonguyen-agilityio/bobix/releases/latest/download/bobix-linux-arm64.deb) |

Not sure which Mac you have? Apple menu → About This Mac → "Chip".

## 2. Install and open it

### macOS

1. Open the `.dmg` and drag **Bobix** into **Applications**.
2. The build is not notarized by Apple, so the first launch is blocked. Either:
   - right-click the app in Applications → **Open** → **Open**, or
   - run `xattr -cr "/Applications/Bobix.app"` in Terminal, then open it normally.
3. Bobix lives in the menu bar (the little creature icon). It has no Dock icon.

### Windows

1. Run `Bobix-windows-x64.exe`. It installs for your user only, no admin rights needed.
2. The installer is not code-signed, so SmartScreen may say "Windows protected your PC". Click **More info → Run anyway**.
3. Bobix lives in the system tray (bottom-right, maybe under the ^ arrow).

### Linux

Install the `.deb`, then open **Bobix** from your apps menu (or run `bobix`).

```bash
sudo apt install ./bobix-linux-amd64.deb
```

Other Linux distributions aren't packaged yet.

- Transparency needs a compositing desktop (GNOME, KDE, and most others do this by default).
- To let a click on a pet bring its terminal forward, install xdotool: `sudo apt install xdotool`.
- On Wayland, Bobix restarts itself through XWayland so the pets stay on top. Set `BOBIX_WAYLAND=1` to keep native Wayland.
- Through XWayland, Bobix sees the cursor only while it is on a pet, so **Fade when the cursor comes near** works only right next to a pet.

## Updating

Bobix tells you when a new version is out (Settings → **Tell me about new versions**, on by default) and updates itself, but only when you press **Update**: in a notification (macOS), the tray menu or Settings → App → Updates. It downloads the new version, checks its signature, tells you about held permission requests and running ticket flows, then installs and restarts. **Check now** (Settings → App → Updates) looks right away, also with the daily check off. Nothing about you is sent: see [Privacy](privacy.md).

- **Test builds** (`-rc.N`) are offered only when Settings → **Get test builds** is on (off by default). Turning it off never installs an older version.
- **macOS:** open Bobix from **Applications**. From the disk image, or from the folder a browser downloaded it to, it cannot replace itself and says so. The updated app opens without the "not notarized" warning; only the first, hand-made install shows it. If Bobix is in a folder only an administrator can write to, macOS asks for your password.
- **Windows:** the installer runs for your user, shows a progress bar, and Bobix starts again.
- **Linux:** the `.deb` is installed by your package manager, which asks for your password (a system prompt: `pkexec`, or `zenity` / `kdialog`). Where none is installed, or you decline, Bobix shows the release page link instead.
- **Versions before the first one with the updater** (0.1.x and earlier test builds): install that first version by hand, as in step 1. From then on Bobix updates itself.
- If an update fails (no network, a download that stops, a file that fails its signature check), nothing is installed and the version you have keeps working; the message says which. The link to the release page is always there.

## 3. Connect Claude Code

The control panel opens on **Settings → Connect Claude Code** the first time.

1. Click **Install hooks**. Bobix adds its hooks to `~/.claude/settings.json`. Your other hooks and settings are kept, and a backup of the file is saved next to it.
2. Send a prompt in any Claude Code session. Its pet appears along the bottom of your screen. Sessions that were already open show up with their next prompt.
3. Click a pet to open its session.

**Send a test event** shows a pretend session if you want to see the pets without using Claude.

## 4. Make it yours

Open the panel (click the tray / menu bar icon → **Open Bobix**) → **Settings**:

- Pets cover something? Pick the **Top** edge, a smaller **Area** (for example *Right corner*), lower **Size** or **Visibility**, or **Only ones that need me**.
- Want it every day? Turn on **Start when I log in**.
- Too many notifications? Turn off **When a session finishes**, keep **When a session needs approval**.

**Restore defaults** (Settings → Advanced) resets everything except the language, the port and start at login, and can be undone.

## Without Node.js

The hook is a tiny Node.js script. If Node.js isn't installed, Bobix uses `curl` (macOS/Linux) or PowerShell (Windows) instead. Everything works except one thing: clicking a pet opens its details in the panel instead of the session window. Install Node.js and click **Reinstall** in Settings → Connect Claude Code to get it back.

## Troubleshooting

| Problem | Fix |
|---------|-----|
| No pets appear | Settings → Connect Claude Code must say **Connected**. Then send a prompt in a session (open sessions report on their next prompt). Try **Send a test event**. |
| "Port 4747 is busy" at launch | Another program uses that port, or Bobix is already running (check the tray). Quit the other copy, or change the port in Settings → Advanced. |
| Pets stopped reacting after a Node.js update | Settings → Connect Claude Code shows **Node.js moved**. Click **Reinstall**. (The app also repairs this itself at launch when it can.) |
| Connect Claude Code says settings.json is invalid | The file has a JSON syntax error. Bobix never overwrites a file it cannot read; fix the file (or restore a backup), then click **Install hooks**. |
| Nothing at all on a company laptop | Your organization may allow only managed hooks (`allowManagedHooksOnly`). User hooks don't run there, so Bobix receives nothing. Ask your admin. |
| A click doesn't open the session | In Claude desktop the session opens directly. For Terminal / iTerm / VS Code the app is brought forward, not a specific tab. On Linux install xdotool. Where Bobix can't bring it forward (a terminal on Wayland), the panel opens on the session and says where it runs. |

## Uninstall

1. Settings → Connect Claude Code → **Remove hooks**. Only Bobix's hooks are removed.
2. Settings → turn off **Start when I log in**.
3. Quit from the tray / menu bar icon and delete the app (macOS: drag it to the Trash; Windows: Settings → Apps → Bobix → Uninstall; Linux: `sudo apt remove bobix`).
4. Optional cleanup: delete `~/.claude/bobix/`, the backups `~/.claude/settings.json.bak-bobix-*`, and the settings folder:
   - macOS: `~/Library/Application Support/Bobix`
   - Windows: `%APPDATA%\Bobix`
   - Linux: `~/.config/bobix`

# Installing Bobix

This guide is for anyone you share Bobix with. It takes about two minutes.

**You need:** Claude Code (CLI, the Claude desktop app, or an IDE extension) on a recent macOS, Windows 10/11, or Ubuntu / Debian (X11 or Wayland).
Node.js is recommended but not required (see [Without Node.js](#without-nodejs)).

## 1. Get the installer

Download the installer for your computer (the links give the newest stable release, still 0.1.3 while 0.2 is in test builds; for a 0.2 test build pick one from [all releases](https://github.com/baonguyen-agilityio/bobix/releases)), or use the file someone sent you. Install from these files: the repository's source is for contributors.

| Your computer | Download |
|---|---|
| Mac with Apple chip (M1 and later) | [Bobix-mac-arm64.dmg](https://github.com/baonguyen-agilityio/bobix/releases/latest/download/Bobix-mac-arm64.dmg) |
| Mac with Intel chip | [Bobix-mac-x64.dmg](https://github.com/baonguyen-agilityio/bobix/releases/latest/download/Bobix-mac-x64.dmg) |
| Windows 10 / 11 (64-bit) | [Bobix-windows-x64.exe](https://github.com/baonguyen-agilityio/bobix/releases/latest/download/Bobix-windows-x64.exe) |
| Ubuntu / Debian (amd64) | [bobix-linux-amd64.deb](https://github.com/baonguyen-agilityio/bobix/releases/latest/download/bobix-linux-amd64.deb) |
| Ubuntu / Debian (ARM64) | [bobix-linux-arm64.deb](https://github.com/baonguyen-agilityio/bobix/releases/latest/download/bobix-linux-arm64.deb) |

**Phone access (test builds only).** A test build (`-rc` in its name) has **Settings → Phones**, where Bobix can send what waits for you to a phone you pair and take its answers (off until you switch it on; [what it sends](privacy.md)). A stable release does not contain that section. Going from a test build to a stable release: the stable one never reads phone access's keys or files and never connects, so a phone you paired keeps showing the computer as offline. Before you switch, disconnect each phone and turn phone access off in the test build, so the phones are told. You enter the address of a relay there (Bobix has no built-in one): a relay may accept invited computers only, and [relay/README.md](../relay/README.md) explains hosting your own.

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

The hook is a tiny Node.js script. If Node.js isn't installed, Bobix uses `curl` (macOS/Linux) or PowerShell (Windows) instead. Everything works except one thing: opening a session (a pet click, **Open session** in the panel, on the pet's card or in the tray, ⌘K ↵) shows its details or says why instead of bringing its window forward. Install Node.js and click **Reinstall** in Settings → Connect Claude Code to get it back.

## Troubleshooting

| Problem | Fix |
|---------|-----|
| No pets appear | Settings → Connect Claude Code must say **Connected**. Then send a prompt in a session (open sessions report on their next prompt). Try **Send a test event**. |
| "Port 4747 is busy" at launch | Another program uses that port, or Bobix is already running (check the tray). Quit the other copy, or change the port in Settings → Advanced. |
| Pets stopped reacting after a Node.js update | Settings → Connect Claude Code shows **Node.js moved**. Click **Reinstall**. (The app also repairs this itself at launch when it can.) |
| Connect Claude Code says settings.json is invalid | The file has a JSON syntax error. Bobix never overwrites a file it cannot read; fix the file (or restore a backup), then click **Install hooks**. |
| Nothing at all on a company laptop | Your organization may allow only managed hooks (`allowManagedHooksOnly`). User hooks don't run there, so Bobix receives nothing. Ask your admin. |
| A click doesn't open the session | Every place that shows a session has an **Open session** button (Overview, the pet's card, the tray list of sessions waiting for you, ⌘K with ↵), and each says why when it cannot. In Claude desktop the session opens directly; one that has ended opens there too, and an ended terminal session offers a button that copies `claude --resume <id>`. For Terminal / iTerm / VS Code the app is brought forward, not a specific tab. On Linux install xdotool. Where Bobix can't bring it forward (a terminal on Wayland), the panel opens on the session and says where it runs. |

## Uninstall

1. Settings → Connect Claude Code → **Remove hooks**. Only Bobix's hooks are removed.
2. Settings → turn off **Start when I log in**.
3. Quit from the tray / menu bar icon and delete the app (macOS: drag it to the Trash; Windows: Settings → Apps → Bobix → Uninstall; Linux: `sudo apt remove bobix`).
4. Optional cleanup: delete `~/.claude/bobix/`, the backups `~/.claude/settings.json.bak-bobix-*`, and the settings folder:
   - macOS: `~/Library/Application Support/Bobix`
   - Windows: `%APPDATA%\Bobix`
   - Linux: `~/.config/bobix`

## Phone: install the app and pair it

Only for test builds of Bobix (see "Phone access" in section 1). The phone app (iOS 18 or Android 10 and later) is a closed beta for invited people and is not in the App Store or Google Play yet; the way to build it for your own phone is [Try the phone app on a real iPhone](development.md#try-the-phone-app-on-a-real-iphone) and, for Android, [The phone app on Android](development.md#the-phone-app-on-android). You also need a relay both devices can reach ([relay/README.md](../relay/README.md)).

1. On the computer: Settings → Phones. Type the relay's address in **Relay address**, save, and switch on **Let phones connect**. The status says "connected".
2. Press **Pair a phone**. A code appears; it works for 2 minutes.
3. In the phone app, choose to connect a computer and scan the code with the app's own scanner (pairing is meant to start only from the app's own scanner). The app shows the relay's address and says to go on only if you set that relay up yourself or trust whoever did.
4. Back on the computer, the phone's name appears with what it asks for. Tick what it may do (starting a ticket and stopping a session start unticked) and press **Connect**. Nothing works on the phone until you do.
5. The phone asks for Face ID or a fingerprint every time it sends an answer, and when it opens if "Require Face ID / fingerprint to open" is on (the default).

To remove a phone: **Disconnect** in the phone app's Settings, or remove it in Settings → Phones on the computer; either side tells the other, and the phone erases its keys. The phone's Settings also has "Erase and pair again", which erases everything it kept about the computer.

A real iPhone, a real Android phone, push through Apple or Google, and a relay on a public domain have not been tried yet: [roadmap.md](roadmap.md) lists what is verified.

## Phone: notifications on Android

The Bobix phone app on Android receives its notices as data messages through Firebase Cloud Messaging; what Google carries is encrypted, and the phone opens it. Some makers' battery managers (Samsung's "sleeping apps", Xiaomi's autostart and battery saver) stop an app's background work, and a push sent to an app the system has force-stopped is not delivered until the app is opened again. This has not yet been measured on a Samsung or a Xiaomi phone for Bobix: when the experiment is done, its outcome (what arrives after the app is swiped away from the recents screen) belongs here. Until then, if notices stop arriving on such a phone, allow Bobix to run in the background in the phone's battery settings and open the app once; a computer whose phone has not opened the app for 30 days stops sending pushes and says so under Settings → Phones.

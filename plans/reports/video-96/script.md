# #96 Intro and how-to video: script

82 s, 1920×1080, 30 fps, no sound. Captions in Vietnamese. Made with **ak:hyperframes** (HeyGen HyperFrames
`hyperframes@0.7.99`, HTML composition in `source/index.html`, rendered locally with the system Chrome and ffmpeg).

Why HyperFrames: the video is a sequence of real screenshots and one real screen recording with captions on top. That
is plain HTML/CSS, which HyperFrames renders as is (no React project as in Remotion, no template studio checkout as in
html-video), and it uses the Chrome already on this Mac (no browser download).

## Scenes

| # | Time | Picture (all real, see Sources) | Caption |
|---|------|----------------------------------|---------|
| 1 | 0–8 s | Bobix icon and name; the pets walking below | Mỗi phiên Claude Code là **một chú thú pixel** trên màn hình. |
| 2 | 8–17 s | The v0.2.0-rc.4 release page, its 5 installers | **Bước 1:** tải file cài cho máy bạn: Mac, Windows hoặc Linux. → Mở app. Bobix nằm trên thanh menu (Windows: khay hệ thống). |
| 3 | 17–28 s | Kết nối tab: "Chưa kết nối", then "Đã kết nối" after Cài hooks | **Bước 2:** mở Bobix → tab **Kết nối** → bấm **Cài hooks**. → Đã kết nối. Gửi một prompt trong Claude Code là thú cưng hiện ra. |
| 4 | 28–42 s | The pets live: five sessions working; one asks for approval (orange outline, red tag); one finishes (XP, crown, waving flag) | Mỗi phiên một chú thú. **Bấm vào** là mở đúng phiên đó. → Viền cam, nhãn đỏ: Claude **đang chờ bạn duyệt**. → Làm xong: nhận XP, đội vương miện, **phất cờ**. |
| 5 | 42–50 s | Six close-ups from the same recording: hard hat + pencil (editing), + terminal (command), + magnifier (reading), signal lamp (waiting), crown + finish flag (done), nightcap (asleep) | Nhìn trang phục là biết phiên **đang làm gì**. |
| 6 | 50–67 s | Bảng (working day with "Cần bạn duyệt"), Skill, Cài đặt | **Bảng**: việc cần bạn duyệt, việc đang làm, việc hôm nay. → **Skill**: vai trò của bạn và skill của từng dự án. → **Cài đặt**: chỗ thú cưng đi, kiểu hiện, cỡ, Đừng làm phiền… |
| 7 | 67–75 s | The "Cần bạn duyệt" card with Cho phép / Từ chối; Settings › Thông báo (Duyệt ngay trên Bobix, Khi phiên cần duyệt, Khi phiên làm xong) | Bobix báo khi phiên **cần duyệt** hoặc **làm xong**. |
| 8 | 75–82 s | Icon, "Bobix v0.2.0-rc.4 · macOS · Windows · Linux", repo link | Chạy trên máy bạn, **không gửi gì ra ngoài**. |

The four panel tabs (Bảng · Skill · Cài đặt · Kết nối) are all shown: Kết nối in scene 3, the other three in scene 6.

## Sources

- App: the real `develop` build at `cdee897` (0.2.0-rc.4), `cargo build --release` from this worktree, run with a
  scratch home and data folder on port 4791 (never the person's own Bobix, settings or `~/.claude`). For the shots only,
  the build carried the uncommitted `capture/evpatch.py` change in `panel.rs` (panel window size 1440×900, opening tab
  and one page script from env vars); it was reverted afterwards.
- Sessions: five pretend sessions sent through the real `src/hook/hook.js` (`capture/world.sh`), in made-up repos
  under `/Users/Shared/acme`. Issues and pull requests on the Board come from a stand-in `gh`
  (`capture/gh-stand-in.js`, team "acme"); Bobix reads them exactly as it reads the real `gh`.
- Panel (`source/assets/setup-*.png`, `board.png`, `skills.png`, `settings*.png`): the panel window captured with
  `screencapture -l` (`capture/cap-panel.sh`); `capture/seq.js` clicks Cài hooks and the tabs as a person would.
- Pets (`source/assets/pets.mp4`, `c-*.png`): the app's own `/pets` page from the running scratch app, in headless
  Chrome (960×170 CSS px at 2×, about 20 frames a second, real time), `capture/rec-pets.mjs`. Events: an approval
  request at 5 s, a finished turn at 11 s.
- Release page (`source/assets/release.png`): the public GitHub page of v0.2.0-rc.4, headless Chrome
  (`capture/shot-url.mjs`).
- Fonts: the app's own Baloo 2 and Be Vietnam Pro (`src/ui/fonts`, SIL OFL).

## Not shown for real

- **The macOS notification banner.** The scratch build is unsigned and not the installed app; macOS showed no banner
  for it, and a bundled copy only got macOS's "allow notifications" prompt, which was not accepted (a system setting).
  Scene 7 shows the in-app side instead: the approval card with Cho phép / Từ chối and the Settings › Thông báo rows.
- **The pets over a real desktop.** The pets are the real page and real events, recorded on a plain background in
  headless Chrome; filming the screen was avoided (other apps' windows).
- The text balloon over a pet ("Cần bạn duyệt: Bash") does not appear in the recording; the waiting pet shows its
  orange outline and red name tag.

## Rebuild

```bash
cd plans/reports/video-96/source
DO_NOT_TRACK=1 npx -y hyperframes@0.7.99 render -o bobix-intro-96.mp4 -f 30 -q high --video-frame-format png
```

The capture scripts in `capture/` have the paths of the machine they ran on; they are kept as a record of how each
picture was made.

<p align="center"><img src="docs/media/icon.png" width="96" height="96" alt=""></p>

<h1 align="center">Bobix</h1>

<p align="center"><b>Mỗi session Claude Code là một chú thú pixel trên màn hình của bạn.</b><br>
Liếc xuống là biết session nào đang làm, session nào cần bạn, session nào đã xong, không phải mở cửa sổ nào.</p>

<p align="center">
  <a href="https://github.com/baonguyen-agilityio/bobix/releases"><img src="https://img.shields.io/github/v/release/baonguyen-agilityio/bobix?include_prereleases&label=release" alt="Bản phát hành mới nhất"></a>
  <a href="https://github.com/baonguyen-agilityio/bobix/actions/workflows/smoke.yml"><img src="https://img.shields.io/github/actions/workflow/status/baonguyen-agilityio/bobix/smoke.yml?branch=main&label=smoke%20test" alt="Smoke test"></a>
  <img src="https://img.shields.io/badge/platform-macOS%20%C2%B7%20Windows%20%C2%B7%20Linux-6b7280" alt="macOS, Windows, Linux">
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-6b7280" alt="Giấy phép MIT"></a>
</p>

<p align="center"><a href="README.md">English</a> · <b>Tiếng Việt</b></p>

<p align="center"><img src="docs/media/demo.gif" alt="Các chú thú đi dọc mép dưới màn hình: một con nhảy với bong bóng đỏ 'Cần bạn duyệt: Bash', một con hiện 'Xong quest!', bảng điều khiển liệt kê các session theo mức cần chú ý." width="860"></p>

<p align="center"><a href="docs/media/demo.mp4">Xem video demo chất lượng đầy đủ (16 giây)</a></p>

---

## Vì sao có Bobix

Chạy vài session Claude Code cùng lúc thì bạn sẽ phải bấm qua từng cái để xem: còn đang làm không? có kẹt ở bước xin quyền không? xong chưa? Bobix cho mỗi session một chú thú đi dọc mép màn hình để trả lời ngay những câu đó.

| Chú thú… | nghĩa là |
|----------|----------|
| đi lại, bong bóng kiểu *đang chạy lệnh* | Claude đang làm |
| nhảy, bong bóng đỏ *Cần bạn duyệt: Bash* | Claude đang chờ bạn duyệt |
| cười, bong bóng xanh *Xong quest!* | session đã xong lượt |
| ngủ | không có gì xảy ra; ngủ lâu thì tạm rời màn hình |

**Bấm vào chú thú để mở đúng session đó.** Rê chuột lên để xem việc đang làm, trạng thái và lượng context còn lại.

## Tính năng

- **Mỗi session một chú thú**, tạo từ id của session, tên lấy theo tiêu đề session.
- **Bấm để mở** session: vào thẳng session trong app Claude desktop, hoặc đưa cửa sổ Terminal, iTerm, VS Code lên trước.
- **Duyệt ngay trên Bobix**: khi Claude xin quyền, bấm Cho phép, Luôn cho phép hoặc Từ chối trên chú thú, trong bảng điều khiển hoặc trên thông báo (macOS). Không ai trả lời trong thời gian bạn đặt (mặc định 20 giây) thì terminal hỏi như bình thường.
- **Bảng điều khiển** liệt kê mọi session theo mức cần bạn chú ý, kèm câu trả lời cuối và nhật ký các tool đã dùng.
- **Thông báo** khi session cần duyệt hoặc đã xong, có âm thanh nếu muốn.
- **Không che việc của bạn**: chọn màn hình, mép trên hoặc dưới, đoạn mép được đi, kích thước và độ rõ. Chú thú mờ đi khi chuột lại gần, mọi chỗ không phải chú thú đều cho click xuyên qua, con ngủ quá lâu sẽ rời màn hình sau khoảng thời gian bạn chọn.
- **Nhẹ máy**: khoảng 1–2 % CPU khi các chú thú đứng yên, không vẽ gì khi không có con nào, không ghi ổ đĩa ngoài file cài đặt.
- **macOS, Windows và Linux**, tiếng Anh và tiếng Việt.
- **Riêng tư**: chạy hoàn toàn trên máy bạn, không cần tài khoản, không gửi gì ra mạng. Xem [Quyền riêng tư](docs/privacy.md).

## Cài đặt

Tải file cho máy của bạn ở **[bản phát hành mới nhất](https://github.com/baonguyen-agilityio/bobix/releases/latest)**:

| Máy của bạn | File |
|---|---|
| Mac chip Apple (M1 trở lên) | `Bobix-<version>-arm64.dmg` |
| Mac chip Intel | `Bobix-<version>.dmg` |
| Windows 10 / 11 (64-bit) | `Bobix.Setup.<version>.exe` |
| Ubuntu / Debian | `bobix_<version>_amd64.deb` (ARM: `_arm64.deb`) |
| Linux khác | `Bobix-<version>.AppImage` (ARM: `-arm64.AppImage`) |

Sau đó:

1. Mở app. Bản build chưa được ký số nên lần đầu mở sẽ có cảnh báo; [hướng dẫn cài đặt](docs/install.md) chỉ cách mở trên từng hệ điều hành.
2. Trong bảng điều khiển, vào **Kết nối → Cài hooks**.
3. Gửi một prompt trong session Claude Code bất kỳ. Chú thú của nó sẽ xuất hiện.

[Hướng dẫn cài đặt](docs/install.md) (tiếng Anh) có thêm phần xử lý sự cố và gỡ cài đặt.

## Ảnh chụp

<table>
<tr>
<td width="50%"><img src="docs/media/settings.png" alt="Cài đặt: xem trước chỗ chú thú đi, cùng các tuỳ chọn mép, vùng, kích thước, độ rõ và những con được hiện."></td>
<td width="50%"><img src="docs/media/setup.png" alt="Kết nối: trạng thái kết nối và ba bước bắt đầu."></td>
</tr>
<tr>
<td><b>Cài đặt</b> có xem trước chỗ chú thú đi.</td>
<td><b>Kết nối</b> Claude Code chỉ với một cú bấm.</td>
</tr>
</table>

## Cách hoạt động

```
Claude Code ──hooks──▶ node ~/.claude/bobix/hook.js ──127.0.0.1──▶ Bobix ──▶ chú thú · bảng điều khiển · thông báo
                                                                              └─ đọc transcript của session để lấy tiêu đề, câu trả lời cuối và context còn lại
```

- Dùng các hook có trong tài liệu của Claude Code: SessionStart, UserPromptSubmit, PreToolUse, PostToolUse, PermissionRequest, Notification, Stop, SessionEnd. Hook chạy bất đồng bộ nên Claude không bao giờ phải chờ.
- Khi cài, Bobix thêm mục vào `~/.claude/settings.json`, giữ nguyên mọi hook và cài đặt khác, và lưu bản backup trước. Khi gỡ, chỉ các mục của Bobix bị xoá.
- Bộ chuyển tiếp chỉ gửi những gì chú thú hiển thị (tên tool, đường dẫn file hoặc lệnh, prompt của bạn), không bao giờ gửi nội dung file hay kết quả tool.
- Bấm Esc hoặc từ chối quyền sẽ kết thúc lượt mà không có hook nào; Bobix nhận ra điều đó trong transcript và cho chú thú nghỉ.

## Câu hỏi thường gặp

**Có làm Claude chậm đi không?** Không. Hook chạy bất đồng bộ và bộ chuyển tiếp thoát sau vài chục mili giây.

**Dùng được với những cách chạy Claude Code nào?** CLI trong mọi terminal, extension cho IDE và app Claude desktop. Bấm để mở sẽ vào thẳng session trong app desktop; ở chỗ khác thì đưa cửa sổ app lên trước, không chọn được đúng tab.

**Không có Node.js thì sao?** Hook chuyển sang dùng `curl` hoặc PowerShell. Mọi thứ vẫn chạy, trừ bấm để mở: khi đó app hiện chi tiết session.

**Máy công ty không hiện gì cả.** Tổ chức của bạn có thể chỉ cho phép hook do quản trị cài (`allowManagedHooksOnly`). Bobix tôn trọng thiết lập đó nên không nhận được sự kiện.

**Code của tôi có bị gửi đi đâu không?** Không. Xem [Quyền riêng tư](docs/privacy.md).

## Tình trạng và giới hạn

- Phiên bản 0.1. Đã thử trên macOS (chip Apple) và Ubuntu (Wayland) với app Claude desktop và CLI; Windows được build và chạy smoke test trên CI.
- Trên Linux Wayland, Bobix chạy qua XWayland để chú thú luôn nằm trên cùng; đưa terminal lên trước cần X11 và `xdotool`.
- Mở session trong app Claude desktop dùng một đường link do app đó đăng ký (`claude://code/continue`), không phải API có tài liệu, nên có thể thay đổi.
- "Context còn lại" là ước tính từ lượng token đã dùng.

## Build từ mã nguồn

Cần Node.js 22 trở lên.

```bash
git clone https://github.com/baonguyen-agilityio/bobix.git && cd bobix
npm install
npm start            # chạy app
npm test             # unit test
npm run dist:mac     # hoặc dist:win, dist:linux: file cài nằm trong thư mục dist
```

Đẩy một tag như `v0.1.0` sẽ chạy [workflow release](.github/workflows/release.yml), build trên từng hệ điều hành và đính kèm file cài vào GitHub Release; tag có dấu gạch (`v0.2.0-rc.1`) sẽ thành bản pre-release. `node scripts/record-demo.mjs` quay lại video demo và ảnh chụp trong `docs/media/`.

## Giấy phép và ghi công

MIT, xem [LICENSE](LICENSE). Font: Baloo 2 và Be Vietnam Pro, giấy phép SIL Open Font License, đóng gói qua Fontsource (giấy phép nằm trong `src/ui/fonts/`).

Bobix là dự án độc lập, không liên kết với Anthropic và không được Anthropic bảo trợ. "Claude" và "Claude Code" là nhãn hiệu của Anthropic.

# gen.py: writes index.html (the HyperFrames composition) from the scene list below. python3 gen.py
# Every picture is a real capture (see ../script.md). A scene: start, length, its pictures, and its captions [(offset, html)].
import json

S = []  # scenes
def scene(t, d, kicker, items, caps): S.append(dict(t=t, d=d, k=kicker, items=items, caps=caps))
# picture helpers: crop of a still (cx, cy, cw, ch in the picture's pixels) placed at (x, y) scaled by s; appears at offset `at`
def crop(src, cx, cy, cw, ch, x, y, s=1.0, at=0.0, out=None, pix=False, flat=False):
    return dict(kind='crop', src=src, cx=cx, cy=cy, cw=cw, ch=ch, x=x, y=y, s=s, at=at, out=out, pix=pix, flat=flat)
def vid(src, x, y, w, h, ms=0.0, at=0.0, out=None, dur=None):
    return dict(kind='vid', src=src, x=x, y=y, w=w, h=h, ms=ms, at=at, out=out, dur=dur)
def tiles(at=0.0): return dict(kind='tiles', at=at)
def html(h, at=0.0, out=None): return dict(kind='html', h=h, at=at, out=out)
C = lambda w, s=1.0: int((1920 - w * s) / 2)  # x that centres a picture w wide at scale s

# ---- intro
scene(0, 6, '', [
  html('<div class="title"><img src="assets/icon.png" alt=""><div><div class="brand">A day of work with Bobix</div><div class="sub">Many Claude Code sessions. One place to see and steer them.</div></div></div>'),
  vid('pets-busy.mp4', 0, 560, 1920, 390, ms=0)],
  [(0.6, 'Filmed in Bobix 0.2.0-rc.4 with real Claude Code sessions on the <b>bobix</b> repo.')])

# ---- 1. See everything
scene(6, 9, '1 · See everything', [vid('pets-busy.mp4', 0, 250, 1920, 390, ms=5)],
  [(0.3, 'Seven Claude Code sessions on the bobix repo. <b>One pet each.</b>'),
   (4.6, 'One project, one creature, <b>a colour per session</b>.')])
scene(15, 8, '1 · See everything', [tiles()],
  [(0.5, 'The costume says what each session is <b>doing right now</b>.')])
scene(23, 11, '1 · See everything', [vid('pets-battery.mp4', 0, 150, 1920, 780, ms=4, out=5.6),
                                      crop('card85.png', 0, 0, 960, 874, C(960), 70, at=5.6, flat=True)],
  [(0.3, '#85 has read a lot: under 25% context left, <b>the battery alarm</b> goes off.'),
   (5.8, 'Point at the pet: its card shows <b>context left: 14%</b>.')])
scene(34, 7, '1 · See everything', [vid('pets-rest.mp4', 0, 260, 1920, 360, ms=2)],
  [(0.3, 'A session on an issue\'s branch shows <b>#88, #86, #84, #17</b> on its pet…'),
   (3.6, '…and Bobix names the session after the issue: <b>[BOB-88]</b>.')])

# ---- 2. Control without opening windows
scene(41, 10, '2 · Control without opening windows', [crop('approvals.png', 128, 172, 1176, 244, C(1176, 1.45), 150, 1.45)],
  [(0.3, 'Two sessions wait for you. <b>The one waiting longest is on top.</b>'),
   (5.0, 'A request that <b>deletes or overwrites</b> data is marked.')])
scene(51, 9, '2 · Control without opening windows', [vid('ov-allow.mp4', C(1170), 50, 1170, 894, ms=2.6)],
  [(0.3, 'Answer on the pet itself: <b>Allow</b> or <b>Deny</b>.'),
   (4.8, 'Allowed: Claude carries on and <b>runs the command</b>.')])
scene(60, 9, '2 · Control without opening windows', [
    crop('deny-before.png', 128, 172, 1176, 140, C(1176, 1.4), 120, 1.4, out=3.6),
    crop('terminal-17.png', 0, 30, 597, 230, C(597, 2.2), 70, 2.2, at=3.6)],
  [(0.3, 'Or in the panel: <b>Deny</b> the delete…'),
   (3.8, '…and Claude gets the answer: <b>Denied from Bobix</b>.')])
scene(69, 5, '2 · Control without opening windows', [crop('terminal-front.png', 0, 0, 597, 385, C(597, 2.0), 110, 2.0)],
  [(0.3, '<b>Click a pet</b> and its terminal comes forward.')])
scene(74, 9, '2 · Control without opening windows', [vid('ov-ondemand.mp4', 0, 0, 1920, 1080, ms=6, out=4.6),
                                                       crop('od-pop-crop.png', 0, 0, 900, 500, C(900, 1.7), 60, 1.7, at=4.6, flat=True)],
  [(0.3, '<b>Show when needed</b>: a pet floats up only while its session needs you.'),
   (4.8, 'The corner pill lists every session, <b>with Allow / Deny</b>.')])
scene(83, 6, '2 · Control without opening windows', [crop('settings-dnd.png', 522, 425, 760, 92, C(760, 1.7), 110, 1.7),
                                                       crop('od-dnd-crop.png', 490, 250, 400, 48, C(400, 2.4), 470, 2.4, flat=True)],
  [(0.3, '<b>Do not disturb</b>: no pets, no notifications. Requests wait on the Board.')])

# ---- 3. From issue to PR
scene(89, 10, '3 · From issue to PR', [crop('board-pm-mid.png', 110, 96, 1210, 804, C(1210), 118, out=5),
                                         crop('board-pm-2.png', 110, 96, 1210, 804, C(1210), 118, at=5)],
  [(0.3, 'The Board follows the day: <b>In progress</b> with its sessions, <b>To do</b>…'),
   (5.2, '…<b>Waiting for review</b> (PR #99 is open) and <b>Done today</b>.')])
scene(99, 9, '3 · From issue to PR', [crop('start-box2.png', 396, 48, 640, 836, C(640, 1.12), 20, 1.12)],
  [(0.3, 'To do suggests the issue to start next, <b>and a skill for your role</b>.'),
   (4.6, '<b>Start</b> opens a new Claude session with this prompt, named after the issue.')])
scene(108, 6, '3 · From issue to PR', [crop('approvals.png', 128, 826, 610, 60, C(610, 1.9), 110, 1.9), crop('dup88.png', 128, 338, 610, 212, C(610, 1.9), 260, 1.9, at=0.8)],
  [(0.3, 'Two sessions on one issue? <b>Bobix warns you.</b>')])
scene(114, 9, '3 · From issue to PR', [crop('stuck.png', 128, 142, 1176, 448, C(1176, 1.4), 120, 1.4)],
  [(0.3, 'Late afternoon: <b>stuck work</b>, each with <b>its next step</b>.'),
   (4.6, 'Done but no PR? <b>Open PR</b>. A PR nobody reviewed? <b>Ask for review</b>.')])

# ---- 4. End of day
scene(123, 8, '4 · End of day', [crop('report-2.png', 396, 100, 642, 736, C(642, 1.18), 20, 1.18)],
  [(0.3, 'Today\'s report, per project, with <b>how long Claude waited for you</b>.')])
scene(131, 7, '4 · End of day', [crop('standup.png', 396, 100, 640, 740, C(640, 1.18), 20, 1.18, out=3.4),
                                  crop('standup-2.png', 396, 100, 640, 740, C(640, 1.18), 20, 1.18, at=3.4)],
  [(0.3, 'A <b>stand-up</b> to copy: done yesterday, in progress today…'),
   (3.6, '…and what is <b>stuck</b>. Bobix sends nothing.')])
scene(138, 7, '4 · End of day', [crop('board-pm-1.png', 128, 642, 1176, 176, C(1176, 1.35), 140, 1.35, out=3.2),
                                  crop('habits.png', 522, 60, 760, 840, C(760, 1.12), 20, 1.12, at=3.2)],
  [(0.3, '<b>Habits</b>: after about five working days, suggestions for what to do next.'),
   (3.4, 'What Bobix keeps is listed, <b>on this machine only</b>.')])
scene(145, 6, '', [html('<div class="end"><img src="assets/icon.png" alt=""><div class="brand">Bobix</div><div class="sub">v0.2.0-rc.4 · macOS · Windows · Linux</div><div class="url">github.com/baonguyen-agilityio/bobix</div></div>')],
  [(0.6, '<b>Everything stays on your machine.</b> No account, nothing sent.')])

TOTAL = S[-1]['t'] + S[-1]['d']
TILES = [('c-read', 'Reading'), ('c-bash', 'Running a command'), ('c-edit', 'Editing code'),
         ('c-wait', 'Needs you'), ('c-done', 'Done'), ('c-rest', 'Resting')]

sections, videos, js = [], [], []
for n, sc in enumerate(S, 1):
    t, d, sid = sc['t'], sc['d'], f's{n}'
    inner = []
    if sc['k']: inner.append(f'<div class="kicker">{sc["k"]}</div>')
    for j, it in enumerate(sc['items']):
        eid = f'{sid}-i{j}'
        if it['kind'] == 'crop':
            pix = ';image-rendering:pixelated' if it['pix'] else ''
            cls = 'crop flat' if it['flat'] else 'crop'
            inner.append(f'<div class="{cls}" data-layout-allow-overflow id="{eid}" style="left:{it["x"]}px;top:{it["y"]}px;width:{it["cw"]}px;height:{it["ch"]}px;'
                         f'transform:scale({it["s"]});transform-origin:left top"><img src="assets/{it["src"]}" style="left:-{it["cx"]}px;top:-{it["cy"]}px{pix}" alt=""></div>')
        elif it['kind'] == 'tiles':
            inner.append('<div class="grid6" id="%s">%s</div>' % (eid, ''.join(f'<div class="tile{" hot" if k == "c-wait" else ""}"><img src="assets/{k}.png" alt=""><span>{l}</span></div>' for k, l in TILES)))
            js.append(f'tl.from("#{eid} .tile", {{ ...IN, stagger: 0.12 }}, {t + 0.2});')
            continue
        elif it['kind'] == 'html':
            inner.append(f'<div id="{eid}">{it["h"]}</div>')
        else:  # video: its own clip on track 2, timed by the scene
            vs, vd = t + it['at'], (it['out'] if it['out'] is not None else d) - it['at']
            videos.append(f'<video id="{eid}" class="clip" src="assets/{it["src"]}" data-start="{vs}" data-duration="{vd}" data-media-start="{it["ms"]}" data-track-index="2" muted playsinline '
                        f'style="position:absolute;left:{it["x"]}px;top:{it["y"]}px;width:{it["w"]}px;height:{it["h"]}px;border-radius:16px"></video>')
            continue
        if it['at']: js.append(f'tl.fromTo("#{eid}", {{ opacity: 0 }}, {{ opacity: 1, duration: 0.5 }}, {t + it["at"]});')
        else: js.append(f'tl.from("#{eid}", {{ opacity: 0, y: 24, duration: 0.6, ease: "power2.out" }}, {t + 0.05});')
        if it['out'] is not None: js.append(f'tl.to("#{eid}", {{ opacity: 0, duration: 0.4 }}, {t + it["out"] - 0.2});')
    for c, (o, h) in enumerate(sc['caps']):
        cid = f'{sid}-c{c}'
        hid = ' style="opacity:0"' if c else ''
        inner.append(f'<div class="cap" id="{cid}"{hid}>{h}</div>')
        if c == 0: js.append(f'tl.from("#{cid}", IN, {t + o});')
        else: js.append(f'tl.to("#{sid}-c{c-1}", {{ opacity: 0, duration: 0.35 }}, {t + o - 0.2}); tl.fromTo("#{cid}", {{ opacity: 0 }}, {{ opacity: 1, duration: 0.45 }}, {t + o});')
    if sc['k']: js.append(f'tl.from("#{sid} .kicker", {{ opacity: 0, duration: 0.5 }}, {t + 0.05});')
    sections.append(f'<section id="{sid}" class="clip scene" data-start="{t}" data-duration="{d}" data-track-index="0">\n        ' + '\n        '.join(inner) + '\n      </section>')

open('index.html', 'w').write(open('index.tpl').read().replace('{{TOTAL}}', str(TOTAL))
    .replace('{{BODY}}', '\n      '.join(sections + videos)).replace('{{JS}}', '\n      '.join(js)))
print('index.html', TOTAL, 's')

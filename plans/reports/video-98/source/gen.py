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
  [(0, 'Filmed in Bobix 0.2.0-rc.4 with real Claude Code sessions on the <b>bobix</b> repo.')])

# ---- 1. See everything (pets page recorded at 1.5x, shown 1:1)
scene(6, 9, '1 · See everything', [vid('pets-busy.mp4', 0, 250, 1920, 390, ms=5)],
  [(0, 'Seven Claude Code sessions on the bobix repo. <b>One pet each.</b>'),
   (4.6, 'One project, one creature, <b>a colour per session</b>.')])
scene(15, 8, '1 · See everything', [tiles()],
  [(0, 'The costume says what each session is <b>doing right now</b>.')])
scene(23, 11, '1 · See everything', [vid('pets-battery.mp4', 80, 220, 960, 390, ms=4),
                                      crop('card85.png', 0, 0, 530, 344, 1200, 250, at=5.6, flat=True)],
  [(0, '#85 has read a lot: under 25% context left, <b>the battery alarm</b> goes off.'),
   (5.6, 'Point at the pet: its card shows <b>context left: 21%</b>.')])
scene(34, 7, '1 · See everything', [vid('pets-rest.mp4', 160, 300, 1600, 300, ms=2)],
  [(0, 'A session on an issue\'s branch shows <b>#88, #86, #84, #17</b> on its pet…'),
   (3.6, '…and Bobix names the session after the issue: <b>[BOB-88]</b>.')])

# ---- 2. Control without opening windows (panel drawn at 1.5x zoom, pets window at 1.6x scale)
scene(41, 10, '2 · Control without opening windows', [crop('approvals.png', 310, 240, 1765, 365, C(1765), 260)],
  [(0, 'Two sessions wait for you. <b>The one waiting longest is on top.</b>'),
   (5.0, 'A request that <b>deletes or overwrites</b> data is marked.')])
scene(51, 9, '2 · Control without opening windows', [vid('ov-allow.mp4', 160, 280, 1600, 314, ms=2.6)],
  [(0, 'Answer on the pet itself: <b>Allow</b> or <b>Deny</b>.'),
   (4.4, 'Allowed: Claude carries on and <b>reads the web page</b>.')])
scene(60, 9, '2 · Control without opening windows', [
    crop('approvals.png', 310, 440, 1765, 140, C(1765), 330, out=3.6),
    crop('terminal-17.png', 0, 0, 1200, 560, C(1200), 160, at=3.6)],
  [(0, 'Or in the panel: <b>Deny</b> the delete…'),
   (3.6, '…and Claude gets the answer: <b>Denied from Bobix</b>.')])
scene(69, 5, '2 · Control without opening windows', [crop('terminal-front.png', 0, 0, 1200, 780, C(1200), 110)],
  [(0, '<b>Click a pet</b> and Terminal comes to the front.')])
scene(74, 9, '2 · Control without opening windows', [
    crop('settings-od.png', 910, 385, 1110, 205, C(1110), 300, out=2.8),
    vid('od.mp4', 0, -150, 1920, 1080, ms=1, at=2.8, out=6.2),
    crop('od-pop.png', 1140, 350, 780, 720, 1080, 150, at=6.2, flat=True)],
  [(0, '<b>Show when needed</b>: a pet floats up only while its session needs you.'),
   (6.2, 'A pill in the corner lists <b>every session</b>.')])
scene(83, 6, '2 · Control without opening windows', [crop('settings-dnd.png', 900, 1194, 1110, 116, C(1110), 230),
                                                       crop('od-dnd.png', 1100, 990, 800, 75, C(800), 470, flat=True)],
  [(0, '<b>Do not disturb</b>: no pets, no notifications. Requests wait on the Board.')])

# ---- 3. From issue to PR
scene(89, 10, '3 · From issue to PR', [crop('board-pm-mid.png', 310, 435, 1765, 820, C(1765), 110, out=5),
                                         crop('board-pm-2.png', 310, 700, 1765, 620, C(1765), 180, at=5)],
  [(0, 'The Board follows the day: <b>In progress</b> with its sessions, <b>To do</b>…'),
   (5, '…<b>Waiting for review</b> (PR #99 is open) and <b>Done today</b>.')])
scene(99, 9, '3 · From issue to PR', [crop('start-box.png', 712, 58, 956, 902, C(956), 12)],
  [(0, 'To do suggests the issue to start next, <b>and a skill for your role</b>.'),
   (4.6, '<b>Start</b> opens a new Claude session with this prompt, named after the issue.')])
scene(108, 6, '3 · From issue to PR', [crop('dup88.png', 310, 455, 1765, 345, C(1765), 250)],
  [(0, 'Two sessions on one issue? <b>Bobix warns you.</b>')])
scene(114, 9, '3 · From issue to PR', [crop('stuck.png', 310, 245, 1765, 395, C(1765), 250)],
  [(0, 'Late afternoon: <b>stuck work</b>, each with <b>its next step</b>.'),
   (4.6, 'A PR nobody reviewed? <b>Ask for review</b>. A branch with no PR? <b>Open PR</b>.')])

# ---- 4. End of day
scene(123, 8, '4 · End of day', [crop('report.png', 713, 268, 960, 800, C(960), 110)],
  [(0, 'Today\'s report, per project, with <b>how long Claude waited for you</b>.')])
scene(131, 7, '4 · End of day', [crop('standup.png', 713, 268, 960, 800, C(960), 110, out=3.4),
                                  crop('standup-2.png', 713, 268, 960, 800, C(960), 110, at=3.4)],
  [(0, 'A <b>stand-up</b> to copy: done yesterday, in progress today…'),
   (3.4, '…and what is <b>stuck</b>. Bobix sends nothing.')])
scene(138, 7, '4 · End of day', [crop('habits-card.png', 310, 345, 1765, 255, C(1765), 300, out=3.2),
                                  crop('habits.png', 900, 120, 1140, 830, C(1140), 100, at=3.2)],
  [(0, '<b>Habits</b>: after about five working days, suggestions for what to do next.'),
   (3.2, 'What Bobix keeps is listed, <b>on this machine only</b>.')])
scene(145, 6, '', [html('<div class="end"><img src="assets/icon.png" alt=""><div class="brand">Bobix</div><div class="sub">v0.2.0-rc.4 · macOS · Windows · Linux</div><div class="url">github.com/baonguyen-agilityio/bobix</div></div>')],
  [(0, '<b>Everything stays on your machine.</b> No account, nothing sent.')])

TOTAL = S[-1]['t'] + S[-1]['d']
TILES = [('c-read', 'Reading'), ('c-bash', 'Running a command'), ('c-edit', 'Editing code'),
         ('c-wait', 'Needs you'), ('c-done', 'Done'), ('c-rest', 'Resting')]

sections, videos, js = [], [], []
# Hard cuts only: a picture is on from its offset to its end, a caption from its offset to the next one; the first
# caption shows from the scene's first frame, so no frame is without one and none is half faded.
hide = lambda at: ' style="opacity:0"' if at else ''
for n, sc in enumerate(S, 1):
    t, d, sid = sc['t'], sc['d'], f's{n}'
    inner = []
    if sc['k']: inner.append(f'<div class="kicker">{sc["k"]}</div>')
    for j, it in enumerate(sc['items']):
        eid = f'{sid}-i{j}'
        if it['kind'] == 'crop':
            cls = 'crop flat' if it['flat'] else 'crop'
            inner.append(f'<div class="{cls}" data-layout-allow-overflow id="{eid}" style="left:{it["x"]}px;top:{it["y"]}px;width:{it["cw"]}px;height:{it["ch"]}px;'
                         f'transform:scale({it["s"]});transform-origin:left top{";opacity:0" if it["at"] else ""}"><img src="assets/{it["src"]}" style="left:-{it["cx"]}px;top:-{it["cy"]}px" alt=""></div>')
        elif it['kind'] == 'tiles':
            inner.append('<div class="grid6" id="%s">%s</div>' % (eid, ''.join(f'<div class="tile{" hot" if k == "c-wait" else ""}"><img src="assets/{k}.png" alt=""><span>{l}</span></div>' for k, l in TILES)))
        elif it['kind'] == 'html':
            inner.append(f'<div id="{eid}"{hide(it["at"])}>{it["h"]}</div>')
        else:  # video: its own clip on track 2, timed by the scene
            vs, vd = t + it['at'], (it['out'] if it['out'] is not None else d) - it['at']
            videos.append(f'<video id="{eid}" class="clip" src="assets/{it["src"]}" data-start="{vs}" data-duration="{vd}" data-media-start="{it["ms"]}" data-track-index="2" muted playsinline '
                          f'style="position:absolute;left:{it["x"]}px;top:{it["y"]}px;width:{it["w"]}px;height:{it["h"]}px"></video>')
            continue
        if it.get('at'): js.append(f'tl.set("#{eid}", {{ opacity: 1 }}, {t + it["at"]});')
        if it.get('out') is not None: js.append(f'tl.set("#{eid}", {{ opacity: 0 }}, {t + it["out"]});')
    for c, (o, h) in enumerate(sc['caps']):
        cid = f'{sid}-c{c}'
        inner.append(f'<div class="cap" id="{cid}"{hide(c)}>{h}</div>')
        if c: js.append(f'tl.set("#{sid}-c{c-1}", {{ opacity: 0 }}, {t + o}); tl.set("#{cid}", {{ opacity: 1 }}, {t + o});')
    sections.append(f'<section id="{sid}" class="clip scene" data-start="{t}" data-duration="{d}" data-track-index="0">\n        ' + '\n        '.join(inner) + '\n      </section>')

open('index.html', 'w').write(open('index.tpl').read().replace('{{TOTAL}}', str(TOTAL))
    .replace('{{BODY}}', '\n      '.join(sections + videos)).replace('{{JS}}', '\n      '.join(js)))
print('index.html', TOTAL, 's')

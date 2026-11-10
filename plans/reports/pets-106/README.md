# New Bobix pets: prototype (#106)

Five original pets, with Petdex (petdex.dev) used only as a style reference: chibi characters and objects with faces,
acting each state with the whole body. No Petdex pet, no known character, no Petdex art in the repo. No app change.

- `index.html`: the prototype, self-contained, open it in a browser. A state viewer (pick a pet, pick a state) and the
  new pets next to 3 of today's pets, at 56 px (today's default) and 84 px (1.5×), one state per row.
- `pets-106.gif`: the two comparison tables, 4.8 s loop, 10 fps, captured at 2× (Retina), 2.8 MB.
- `contact-sheet-1fps.png`: the same tables at 1× (true size on a 1× screen), t = 0, 1, 2, 3, 4 s left to right.
- `state-viewer.png`: the state viewer (Nota, waiting).

## The pets

| Pet | Idea |
|-----|------|
| **Nota** | A spiral notebook that writes down everything you do. Face on the cover, a label, a red bookmark ribbon. |
| **Tty** | A little terminal. Its face is on the screen: green prompt eyes, a blinking cursor for a mouth. Red eyes when it fails, dim when asleep. |
| **Bitbug** | A small teal beetle, the bug you are hunting, friendly about it. Antennae wave when it is happy, droop when it is not. |
| **Dux** | A rubber duck, for rubber-duck debugging. The beak opens when it talks to you. |
| **Hoot** | A night owl who codes late. Facial disc, ear tufts, a chevron belly. |

Two animals (Bitbug, Hoot), three coding objects (Nota, Tty, Dux).

## States

Each pet has 9 frame loops (8 + a mirror), the same rows as a Petdex sheet:

| Bobix state | Loop | Frames | What the body does |
|---|---|---|---|
| asleep (idle) | Asleep | 2 × 1200 ms | eyes shut, sinks 1–2 px, breathes; the overlay's "z" floats up |
| walking | Walk right / left | 4 × 150 ms | legs step, body bobs, arms swing, looks where it goes |
| working (edit, command) | Typing | 4 × 150 ms | hands in front, alternating, eyes down |
| working (read, search, web) | Reading | 4 × 300 ms | hands held in front, eyes scan left and right |
| waiting for you | Waiting | 4 × 220 ms | faces you, wide eyes with glints, "o" mouth, hands on hips, taps one foot |
| done | Wave, then jump | 4 + 4 × 150 ms | happy eyes and cheeks, waves; then crouches, jumps with both arms up, sparkles at the peak |
| failed | Failed | 4 × 300 ms | slumps 2 px, arms hang, worried brows, a frown, a drop of sweat |

The jump raises the whole sprite with CSS (like the overlay's hop today) instead of moving it inside its 36×36 grid,
so no frame is ever cut off. A check over every pet, state and frame found no pixel on the grid's edge.

## How they are built (ready to move into the app)

- Same format as today's pets: a 36×36 grid of packed colours (`{ w, h, data }`), drawn by `spriteSVG` from
  `src/ui/creature.js` (one path per colour, crisp edges). Same box on screen, so the line, name tag, balloons and the
  effects in `pet-fx.js` take them as they take today's pets.
- One draw function per pet (the body) plus shared parts: `pose(state, frame)` (offsets, foot lift, arm modes, eyes,
  mouth), `legs`, `arm`, `face`, `outline`. A new pet is about 20 lines.
- 1-pixel dark outline round the body (8 ways, as today), then the arms on top with their own outline (the way
  `costume.js` layers a prop), so an arm in front of the body still reads.
- The page is `index.html` with `src/ui/creature.js` (as of `ed1d76d`) copied in.

## Size

Today's pets are 26–30 of the 36 rows tall; the new ones 32–33, so at the same box they stand a little taller and
hold more detail (a face with brows, cheeks, hands, feet).

- **56 px (recommended default, as today).** On a Retina screen (2×, most Macs) every state reads and the pixels are
  crisp. The pet line keeps its width. On a 1× screen one grid pixel is 1.56 screen pixels, so pixels come out uneven
  and the faces get small, but the poses still read (see the contact sheet).
- **84 px (1.5×).** Every detail reads on any screen, but the line is 1.5× wider and taller and covers more of the
  desktop. Better left to the existing Size setting (32–128 px) for people who want it.
- Worth testing: **72 px**, where one grid pixel is exactly 2 screen pixels on a 1× screen (perfectly crisp
  everywhere), at about 1.3× today's line.

## Costumes (state props) on the new pets

`costume.js` draws a prop into the pet's own 36×36 sprite: a hat at the head's top row (`headTop`), a held tool on the
side the pet walks to. That carries over:

- **Held props** (pencil, terminal, magnifier, globe, flag) go in the hand. The new pets have real hands, and `pose()`
  knows where each hand ends in every frame, so a prop can follow the typing or waving hand instead of a fixed spot.
  The typing and reading poses are made to hold one.
- **Hats** (hard hat, headlamp, crown, nightcap) need a head anchor per pet instead of the first row in the middle:
  Bitbug's antennae and Hoot's tufts would catch a hat found that way. Proposed: each pet gives `head: [x, y, w]`.
- **Tty** could show some props on its screen instead (a spinner while working, a check when done).
- Props stay inside the 36×36 box, as today. The jump lifts the dressed sprite, so the props jump with it.

## Open questions

1. Which pets go into the app? All five, or a pick of them?
2. Colours: today a project's sessions differ by body colour (6 tokens). Do the new pets get tint variants (e.g. a red
   Nota, a green Tty), or does a project's second session get a different pet?
3. Is a pet chosen per project in the panel (like Change pet), or given out by project hash as today's creature is?
4. Default size: keep 56 px, or move to 72 px for exact 2× pixels on 1× screens?
5. Should the app gain a "failed" state (a turn that ended in an error) now that the pets can show one?
6. Do today's generated creatures stay as a choice next to the new pets?

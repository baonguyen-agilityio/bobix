// Motion for the Bobix site. Everything here is optional: without this script, or with reduced motion, the pets stay at
// the bottom of the hero and the demo video waits for a press on play.
const still = matchMedia('(prefers-reduced-motion: reduce)').matches;

// The demo plays by itself, muted and looping, with its controls to pause it.
const demo = document.querySelector('#demo video');
if (demo && !still) { demo.autoplay = true; demo.play().catch(() => {}); }

// The pets on the hero's bottom edge stay pinned to the bottom of the screen once that edge scrolls into view: full
// size at the top and at the end of the page, small and quiet while you read in between (--small goes 0 → 1). While the
// hero's bottom is still below the screen (tall hero on a phone) they ride on it (--drop), so they never cover the hero.
const edge = document.querySelector('.edge'), hero = document.querySelector('.hero');
if (edge && hero && !still) {
  document.documentElement.classList.add('peek');
  const RAMP = 320; // px of scroll over which they shrink (after the top) and grow back (before the end)
  const clamp = v => Math.min(1, Math.max(0, v));
  let queued = false;
  const update = () => {
    queued = false;
    const y = scrollY, end = document.documentElement.scrollHeight - innerHeight;
    const below = hero.getBoundingClientRect().bottom - innerHeight; // > 0: the hero's bottom is still under the screen
    edge.style.setProperty('--drop', `${Math.max(0, below)}px`);
    edge.style.setProperty('--small', Math.min(clamp((-below - 80) / RAMP), clamp((end - y) / RAMP)).toFixed(3));
  };
  addEventListener('scroll', () => { if (!queued) { queued = true; requestAnimationFrame(update); } }, { passive: true });
  addEventListener('resize', update);
  update();
}

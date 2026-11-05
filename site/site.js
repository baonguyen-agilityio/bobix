// Motion for the Bobix site. Everything here is optional: without this script every taskbar pet shows its bubble and
// the demo video waits for a press on play.
const still = matchMedia('(prefers-reduced-motion: reduce)').matches;

// The demo plays by itself, muted and looping, with its controls to pause it.
const demo = document.querySelector('#demo video');
if (demo && !still) { demo.autoplay = true; demo.play().catch(() => {}); }

// The taskbar pets react to what you read: the window closest to the middle of the screen says which one speaks
// (its data-say: work, wait, done; family and idle let the balloons and the sleeper speak for themselves).
const wins = [...document.querySelectorAll('.win[data-say]')];
if (wins.length && 'IntersectionObserver' in window) {
  const body = document.body;
  body.classList.add('talk');
  body.dataset.say = wins[0].dataset.say;
  const seen = new IntersectionObserver(entries => {
    for (const e of entries) if (e.isIntersecting) body.dataset.say = e.target.dataset.say;
  }, { rootMargin: '-45% 0px -45% 0px' }); // a thin band across the middle of the screen
  wins.forEach(w => seen.observe(w));
}

// Evidence-only panel sequencer: does what a person would click, on a fixed clock from page load.
addEventListener('DOMContentLoaded', () => {
  const at = (s, f) => setTimeout(f, s * 1000), q = s => document.querySelector(s);
  at(7, () => q('[data-act="install"]')?.click());
  at(12, () => q('.tab[data-tab="sessions"]').click());
  at(26, () => q('.tab[data-tab="skills"]').click());
  at(32, () => q('.tab[data-tab="settings"]').click());
});

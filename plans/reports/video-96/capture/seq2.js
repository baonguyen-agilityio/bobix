// Evidence-only: Settings, scrolled so its "Thông báo" (notifications) section is at the top.
addEventListener('DOMContentLoaded', () => setTimeout(() => {
  const h = [...document.querySelectorAll('#settings .sec h2')].find(e => e.textContent.trim() === 'Thông báo');
  h?.closest('.sec')?.scrollIntoView({ block: 'start' });
}, 6000));

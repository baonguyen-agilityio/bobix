<!doctype html>
<html lang="en" data-resolution="landscape">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=1920, height=1080" />
    <title>A day of work with Bobix</title>
    <script src="https://cdn.jsdelivr.net/npm/gsap@3.14.2/dist/gsap.min.js"></script>
    <link rel="stylesheet" href="assets/fonts/fonts.css" />
    <style>
      * { margin: 0; padding: 0; box-sizing: border-box; }
      html, body { width: 1920px; height: 1080px; overflow: hidden; background: #f6f0e6; }
      body { font-family: "Be Vietnam Pro", sans-serif; color: #2b2622; }
      .scene { position: absolute; inset: 0; background: #f6f0e6; }
      .brand { font-family: "Baloo 2", sans-serif; font-weight: 700; }
      /* caption: one line, always in the bottom band, above everything */
      .cap { position: absolute; z-index: 5; left: 50%; bottom: 40px; transform: translateX(-50%); max-width: 1720px; width: max-content;
        padding: 16px 38px; border-radius: 22px; background: #2b2622; color: #fff; font: 600 42px/1.3 "Be Vietnam Pro", sans-serif; text-align: center; }
      .cap b { color: #ffb38a; font-weight: 600; }
      .kicker { position: absolute; z-index: 5; left: 48px; top: 30px; padding: 6px 20px; border-radius: 14px; background: #fffaf2; border: 2px solid #eadfcd;
        font: 700 34px "Baloo 2", sans-serif; color: #c4552f; }
      /* a part of a real screenshot: the box is the part's size, the picture is moved so the part shows */
      .crop { position: absolute; overflow: hidden; border-radius: 12px; background: #fff; box-shadow: 0 2px 0 #e3d9c9, 0 24px 60px rgba(60, 40, 20, .18); }
      .crop img { position: absolute; display: block; }
      .crop.flat { background: none; box-shadow: none; border-radius: 0; }
      .grid6 { position: absolute; left: 50%; top: 120px; transform: translateX(-50%); display: grid; grid-template-columns: repeat(3, 440px); gap: 26px 40px; }
      .tile { background: #fffaf2; border: 2px solid #eadfcd; border-radius: 22px; padding: 6px 0 12px; text-align: center; }
      .tile img { width: 384px; height: 320px; image-rendering: pixelated; display: block; margin: 0 auto; }
      .tile span { font: 600 34px "Be Vietnam Pro", sans-serif; }
      .tile.hot { border-color: #e7744f; background: #fff3ec; }
      .title { position: absolute; left: 50%; top: 150px; transform: translateX(-50%); display: flex; align-items: center; gap: 40px; width: max-content; }
      .title img { width: 170px; height: 170px; image-rendering: pixelated; }
      .title .brand { font-size: 104px; line-height: 1; }
      .title .sub, .end .sub { font: 500 40px "Be Vietnam Pro", sans-serif; color: #6b6158; margin-top: 12px; }
      .end { position: absolute; left: 50%; top: 150px; transform: translateX(-50%); text-align: center; width: max-content; }
      .end img { width: 190px; height: 190px; image-rendering: pixelated; }
      .end .brand { font-size: 120px; line-height: 1.05; }
      .end .url { font: 500 40px "Be Vietnam Pro", sans-serif; color: #c4552f; margin-top: 26px; }
    </style>
  </head>
  <body>
    <div id="root" data-composition-id="main" data-start="0" data-duration="{{TOTAL}}" data-width="1920" data-height="1080">
      {{BODY}}
    </div>
    <script>
      window.__timelines = window.__timelines || {};
      const tl = gsap.timeline({ paused: true });
      const IN = { opacity: 0, y: 24, duration: 0.6, ease: "power2.out" };
      {{JS}}
      window.__timelines["main"] = tl;
    </script>
  </body>
</html>

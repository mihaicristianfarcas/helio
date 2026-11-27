//! The app's window alone, cut out of the hero shots (capture-hero.sh), for
//! the pictures the page tilts and frames itself. capture-hero.sh draws a
//! 1120x740 pt window in the middle of a 2700x1800 px canvas at 2x, so the
//! window is the 2240x1480 px rectangle 230 px in and 160 px down. Its
//! rounded corners hold a little wallpaper (about 28 px of it at each). The
//! cards show the window running off their right and bottom edges, so the
//! cut leaves off its last 32 px on those two sides, corners and outline
//! with them; the top left's is covered by the page's own corner
//! (global.css, .shot).
//!
//!   node scripts/crop-windows.mjs
import sharp from 'sharp';

/** The window less its right and bottom 32 px, where its corners are. */
const WINDOW = { left: 230, top: 160, width: 2240 - 32, height: 1480 - 32 };

for (const name of ['hosts', 'agent', 'devices']) {
  const from = new URL(`../src/assets/hero/${name}.webp`, import.meta.url).pathname;
  const to = new URL(`../src/assets/windows/${name}.webp`, import.meta.url).pathname;
  await sharp(from).extract(WINDOW).webp({ quality: 90 }).toFile(to);
  console.log(to);
}

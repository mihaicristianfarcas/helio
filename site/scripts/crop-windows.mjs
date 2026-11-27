//! The app's window alone, cut out of the hero shots (capture-hero.sh), for
//! the pictures the page tilts and frames itself. capture-hero.sh draws a
//! 1120x740 pt window in the middle of a 2700x1800 px canvas at 2x, so the
//! window is the 2240x1480 px rectangle 230 px in and 160 px down; its
//! rounded corners still hold a little wallpaper, which the page's own
//! corners (global.css, .shot) cover.
//!
//!   node scripts/crop-windows.mjs
import sharp from 'sharp';

const WINDOW = { left: 230, top: 160, width: 2240, height: 1480 };

for (const name of ['hosts', 'agent', 'devices']) {
  const from = new URL(`../src/assets/hero/${name}.webp`, import.meta.url).pathname;
  const to = new URL(`../src/assets/windows/${name}.webp`, import.meta.url).pathname;
  await sharp(from).extract(WINDOW).webp({ quality: 90 }).toFile(to);
  console.log(to);
}

//! The site's one indexable page. Section anchors are parts of it, and build
//! dates do not say when its content changed, so neither is listed.
import type { APIRoute } from 'astro';

export const GET: APIRoute = ({ site }) => {
  const home = new URL(import.meta.env.BASE_URL.replace(/\/?$/, '/'), site);
  const pages = [home];
  const urls = pages.map((page) => `  <url><loc>${page.href.replace(/&/g, '&amp;')}</loc></url>`).join('\n');
  return new Response(
    `<?xml version="1.0" encoding="UTF-8"?>\n<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n${urls}\n</urlset>\n`,
    { headers: { 'Content-Type': 'application/xml; charset=utf-8' } },
  );
};

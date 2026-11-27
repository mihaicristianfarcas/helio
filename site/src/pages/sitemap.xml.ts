//! The site's pages: the landing page and each page of the documentation.
//! Section anchors are parts of a page, and build dates do not say when
//! the content changed, so neither is listed.
import type { APIRoute } from 'astro';
import { PAGES, hrefOf } from '../lib/docs.mjs';

export const GET: APIRoute = ({ site }) => {
  const base = import.meta.env.BASE_URL.replace(/\/?$/, '/');
  const pages = [base, ...PAGES.map((page) => hrefOf(base, page.slug))].map((path) => new URL(path, site));
  const urls = pages.map((page) => `  <url><loc>${page.href.replace(/&/g, '&amp;')}</loc></url>`).join('\n');
  return new Response(
    `<?xml version="1.0" encoding="UTF-8"?>\n<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n${urls}\n</urlset>\n`,
    { headers: { 'Content-Type': 'application/xml; charset=utf-8' } },
  );
};

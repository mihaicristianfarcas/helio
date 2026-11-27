# Helio's website

The landing page and the documentation, built with
[Astro](https://astro.build). The landing page shows the app as it is, in
real screenshots, and figures from the benchmarks; the documentation is the
repository's own Markdown (`src/lib/docs.mjs` lists the pages), served at
`/docs/`.

Builds target `https://helio.mihaicristian.dev` by default, including
the canonical URL, link previews, software structured data, `robots.txt`
and `sitemap.xml`. Set `SITE_URL` and `BASE_PATH` when building for
another domain or a project path.

```sh
npm install
npm run dev       # http://localhost:4321
npm run build     # astro check, then site/dist/
npm test          # build, then check the built site
npx wrangler deploy   # after a build: site/dist/ to helio.mihaicristian.dev
```

A push to the `landing-page` branch deploys it by itself
(`.github/workflows/site.yml`), to the Cloudflare Worker that
`wrangler.jsonc` describes.

Tab and Shift+Tab visit the clickable links and buttons in page order, from
the header to the footer, and every download has its own stop. The first
Tab shows Skip to content, which moves focus to the first action in the main
content. On the landing page, a link to one of its sections moves focus to
the first action there, and leaves the address as it was; in the
documentation, a section's link stays in the address, to be shared.

The header stays at the top, with a line under it once something has
scrolled beneath. On refresh, the saved scroll position is restored after
the component scripts have arranged the downloads and the fonts have settled; the font wait has a 600 ms fallback.

Without JavaScript, every download is shown. With it, the downloads show
one button, for the system the browser reports, and list the others under
it. Phones and tablets, where Helio does not run, see no downloads, and a
Download button says why. Copy controls appear when their handlers are
ready, and their results are announced to screen readers.

On a computer's screen each section fits in one screen under the header
(down to 1440x790): the hero is one screen tall, its pictures fading out at
its foot, and the gaps and cards take their size from the screen's height.

`npm test` checks this in a simulated DOM at desktop and phone widths, and
every link of the documentation against the built pages. Browser rendering
and screen-reader checks are separate.

How it is put together and how its pictures are made (`scripts/`):
[docs/development.md](../docs/development.md#the-website).

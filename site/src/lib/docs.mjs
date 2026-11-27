// The documentation pages: the repository's own Markdown (README.md, docs/
// and the three files beside them), each at /docs/SLUG/, grouped as
// docs/README.md groups them. docs/README.md itself is not a page: the
// sidebar is its list. Plain JavaScript, as astro.config.mjs loads the link
// rewriter below before anything is compiled.
import { existsSync, statSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const REPO = 'https://github.com/mihaicristianfarcas/helio';

/** The pages in the sidebar's order, by group. `slug` '' is /docs/ itself. */
export const GROUPS = [
  {
    name: 'Using Helio',
    pages: [
      { file: 'README.md', slug: '', title: 'Overview' },
      { file: 'docs/install.md', slug: 'install', title: 'Installing' },
      { file: 'docs/usage.md', slug: 'usage', title: 'Using Helio' },
      { file: 'docs/cli.md', slug: 'cli', title: 'Command line' },
      { file: 'docs/logs.md', slug: 'logs', title: 'Logs' },
      { file: 'docs/ai-agents.md', slug: 'ai-agents', title: 'AI agents' },
    ],
  },
  {
    name: 'How it works',
    pages: [
      { file: 'docs/architecture.md', slug: 'architecture', title: 'Architecture' },
      { file: 'docs/networking.md', slug: 'networking', title: 'Networking' },
      { file: 'docs/platforms/macos.md', slug: 'platforms/macos', title: 'macOS' },
      { file: 'docs/platforms/windows.md', slug: 'platforms/windows', title: 'Windows' },
      { file: 'docs/platforms/linux.md', slug: 'platforms/linux', title: 'Linux' },
      { file: 'docs/ui.md', slug: 'ui', title: 'The window' },
      { file: 'docs/benchmarks.md', slug: 'benchmarks', title: 'Benchmarks' },
      { file: 'docs/parity.md', slug: 'parity', title: 'Against Moonlight' },
      { file: 'docs/verification.md', slug: 'verification', title: 'Verification' },
    ],
  },
  {
    name: 'Changing Helio',
    pages: [
      { file: 'docs/development.md', slug: 'development', title: 'Development' },
      { file: 'AGENTS.md', slug: 'agents', title: 'Rules for changes' },
      { file: 'CONTRIBUTING.md', slug: 'contributing', title: 'Contributing' },
      { file: 'SECURITY.md', slug: 'security', title: 'Security' },
    ],
  },
];

export const PAGES = GROUPS.flatMap((group) => group.pages.map((page) => ({ ...page, group: group.name })));

/** The globs the content collection loads, from the repository's root. */
export const FILES = PAGES.map((page) => page.file);

/** A page's collection id: its slug, and `index` for /docs/ itself. */
export const idOf = (file) => {
  const page = PAGES.find((p) => p.file === file);
  if (!page) throw new Error(`${file} is not a documentation page (src/lib/docs.mjs)`);
  return page.slug || 'index';
};

/** The path a page is served at, under the site's base. */
export const hrefOf = (base, slug) => `${base.replace(/\/?$/, '/')}docs/${slug ? `${slug}/` : ''}`;

/**
 * The README's first picture is the site's link preview (public/og.jpg),
 * whose two windows the landing page's hero already shows; the Overview
 * page goes without it, and starts on the text.
 */
const PREVIEW = { file: 'README.md', src: 'docs/images/helio.jpg' };

/**
 * A Sätteri hast plugin (Astro's Markdown processor): a link in a page goes
 * where it went on GitHub. One to another page becomes that page here, its
 * #anchor kept (Astro gives headings GitHub's ids); one to anything else in
 * the repository, a source file or a folder, goes to it on GitHub. Pictures
 * are left to Astro, which finds them beside the Markdown, but for the
 * README's link preview (PREVIEW), which is taken out with its paragraph.
 * Astro keeps a page's render (.astro/ and node_modules/.astro/, each a
 * data-store.json) while its Markdown is unchanged, so a change here shows
 * once those are deleted.
 */
export function docLinks({ root, base }) {
  const byFile = new Map(PAGES.map((page) => [page.file, page]));
  const rewrite = (href, from) => {
    if (/^([a-z][a-z0-9+.-]*:|#|\/)/i.test(href)) return href;
    const [target, hash = ''] = href.split('#');
    const to = path.posix.normalize(path.posix.join(path.posix.dirname(from), decodeURI(target)));
    const anchor = hash ? `#${hash}` : '';
    if (to === 'docs/README.md') return hrefOf(base, '') + anchor;
    const page = byFile.get(to);
    if (page) return hrefOf(base, page.slug) + anchor;
    const onDisk = path.join(root, to);
    const kind = existsSync(onDisk) && statSync(onDisk).isDirectory() ? 'tree' : 'blob';
    return `${REPO}/${kind}/main/${encodeURI(to.replace(/\/$/, ''))}${anchor}`;
  };
  // Called once for each document: only the documentation's are changed.
  return ({ fileURL }) => {
    const from = fileURL ? path.relative(root, fileURLToPath(fileURL)).split(path.sep).join('/') : null;
    if (!from || !byFile.has(from)) return null;
    return {
      name: 'helio-doc-links',
      element: {
        filter: ['a', 'img'],
        visit(node, ctx) {
          if (node.tagName === 'img') {
            if (from === PREVIEW.file && node.properties?.src === PREVIEW.src) {
              const paragraph = ctx.parent(node);
              ctx.removeNode(paragraph?.type === 'element' && paragraph.tagName === 'p' ? paragraph : node);
            }
            return;
          }
          const href = node.properties?.href;
          if (typeof href === 'string') ctx.setProperty(node, 'href', rewrite(href, from));
        },
      },
    };
  };
}

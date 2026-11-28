//! Regressions against the built site and its actual component scripts:
//! the landing page's keyboard order, focus, downloads and star count, and
//! the documentation's pages and links. DOM checks cover control order and
//! focus changes; they do not render a browser.
import assert from 'node:assert/strict';
import { existsSync, readdirSync, readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';
import { Window } from 'happy-dom';
import ts from 'typescript';

const site = dirname(dirname(fileURLToPath(import.meta.url)));
const html = readFileSync(join(site, 'dist/index.html'), 'utf8').replace(/<script\b[^>]*>[\s\S]*?<\/script>/gi, '');
const actions = 'a[href], button, summary, input:not([type="hidden"]), select, textarea, video[controls]';

function visible(element, window) {
  for (let parent = element; parent; parent = parent.parentElement) {
    if (parent.hidden || parent.hasAttribute('inert') || parent.getAttribute('aria-hidden') === 'true') return false;
    const style = window.getComputedStyle(parent);
    if (style.display === 'none' || style.visibility === 'hidden') return false;
    if (parent.localName === 'details' && !parent.open && !parent.querySelector('summary')?.contains(element)) return false;
  }
  return true;
}

function page(components = [], width = 1280) {
  const window = new Window({
    url: 'http://localhost/',
    settings: {
      enableJavaScriptEvaluation: true,
      suppressInsecureJavaScriptEnvironmentWarning: true,
      disableJavaScriptFileLoading: true,
      disableCSSFileLoading: true,
    },
  });
  window.happyDOM.setViewport({ width, height: 900 });
  window.fetch = async () => { throw new Error('release data is offline'); };
  window.IntersectionObserver = class { observe() {} };
  window.ResizeObserver = class { observe() {} };
  const document = window.document;
  document.write(html);
  for (const link of document.querySelectorAll('link[rel="stylesheet"]')) {
    const style = document.createElement('style');
    style.textContent = readFileSync(join(site, 'dist', new URL(link.href).pathname), 'utf8');
    document.head.append(style);
  }
  // Supply geometry only for visibility checks. Layout and native key events
  // require a real browser; the assertions below concern DOM focus order.
  window.HTMLElement.prototype.getClientRects = function () {
    return visible(this, window) ? [{ top: 0, bottom: 40, height: 40 }] : [];
  };
  const run = (component) => {
    const source = readFileSync(join(site, 'src', component), 'utf8');
    for (const match of source.matchAll(/<script\b(?![^>]*\/>)([^>]*)>([\s\S]*?)<\/script>/g)) {
      if (match[1].includes('application/ld+json')) continue;
      const js = ts.transpileModule(match[2], { compilerOptions: { target: ts.ScriptTarget.ES2022 } }).outputText;
      window.eval(`(() => { ${js} })()`);
    }
  };
  for (const component of components) run(component);
  const enabled = (element) => visible(element, window)
    && !element.matches(':disabled, [aria-disabled="true"]');
  const stops = () => [...document.querySelectorAll(`${actions}, [tabindex]`)]
    .filter((element) => enabled(element) && Number(element.getAttribute('tabindex') ?? 0) >= 0);
  return { window, document, run, enabled, stops, close: () => window.happyDOM.abort() };
}

const components = ['components/Nav.astro', 'components/Install.astro', 'components/Command.astro', 'layouts/Base.astro'];

test('cached fonts cannot restore a refresh before the downloads initialize', async () => {
  const { window, document, run, close } = page();
  try {
    document.fonts = { ready: Promise.resolve() };
    window.performance.getEntriesByType = () => [{ type: 'reload' }];
    window.sessionStorage.setItem('scroll:/', '400');
    run('layouts/Base.astro');
    await Promise.resolve();
    assert.equal(window.scrollY, 0, 'the layout before the scripts must not receive the saved position');
    assert.equal(document.documentElement.dataset.restore, '400');
    for (const component of components.filter((component) => component !== 'layouts/Base.astro')) run(component);
    assert.equal(document.querySelectorAll('[data-picker] .panel:not([hidden])').length, 1);
    document.dispatchEvent(new window.Event('DOMContentLoaded'));
    await Promise.resolve();
    assert.equal(window.scrollY, 400);
    assert.equal(document.documentElement.hasAttribute('data-restore'), false);
  } finally { await close(); }
});

test('the refresh fallback cannot reveal an unfinished layout while component scripts load', async () => {
  const { window, document, run, close } = page();
  try {
    let fontsReady;
    document.fonts = { ready: new Promise((resolve) => { fontsReady = resolve; }) };
    window.performance.getEntriesByType = () => [{ type: 'reload' }];
    window.sessionStorage.setItem('scroll:/', '400');
    run('layouts/Base.astro');
    await new Promise((resolve) => setTimeout(resolve, 650));
    assert.equal(window.scrollY, 0);
    assert.equal(document.documentElement.dataset.restore, '400');
    for (const component of components.filter((component) => component !== 'layouts/Base.astro')) run(component);
    document.dispatchEvent(new window.Event('DOMContentLoaded'));
    fontsReady();
    await Promise.resolve();
    assert.equal(window.scrollY, 400);
    assert.equal(document.documentElement.hasAttribute('data-restore'), false);
  } finally { await close(); }
});

test('a position restored on refresh shows the line under the header', async () => {
  const { window, document, run, close } = page();
  try {
    let fontsReady;
    document.fonts = { ready: new Promise((resolve) => { fontsReady = resolve; }) };
    window.performance.getEntriesByType = () => [{ type: 'reload' }];
    window.sessionStorage.setItem('scroll:/', '400');
    run('layouts/Base.astro');
    run('components/Nav.astro');
    document.dispatchEvent(new window.Event('DOMContentLoaded'));
    fontsReady();
    await new Promise(setImmediate);
    window.dispatchEvent(new window.Event('scroll'));
    assert.equal(window.scrollY, 400);
    assert.equal(document.querySelector('[data-nav]').hasAttribute('data-stuck'), true);
  } finally { await close(); }
});

test('scrolling keeps the header in place, and its line shows only with something under it', async () => {
  const { window, document, run, close } = page();
  try {
    run('components/Nav.astro');
    const header = document.querySelector('[data-nav]');
    const scrollTo = (y) => {
      window.scrollTo(0, y);
      window.dispatchEvent(new window.Event('scroll'));
    };
    assert.equal(header.hasAttribute('data-stuck'), false);
    scrollTo(400);
    assert.equal(header.hasAttribute('data-stuck'), true);
    scrollTo(0);
    assert.equal(header.hasAttribute('data-stuck'), false);
  } finally { await close(); }
});

for (const width of [1280, 390]) {
  test(`at ${width}px the page offers every visible action, in header-to-footer order`, async () => {
    const { window, document, enabled, stops, close } = page(components, width);
    try {
      const order = stops();
      const expected = [...document.querySelectorAll(actions)].filter(enabled);
      assert.deepEqual(order.map((element) => element.outerHTML), expected.map((element) => element.outerHTML));
      assert.ok(order.length > 25, 'the traversal must cover the whole page');
      assert.equal(document.querySelectorAll('[tabindex]:not(a):not(button):not(input):not(summary)').length, 0);
      assert.equal(document.querySelectorAll('[tabindex]:not([tabindex="0"]):not([tabindex="-1"])').length, 0);
      const phases = order.map((element) => element.closest('header') ? 1
        : element.closest('main') ? 2 : element.closest('footer') ? 3 : 0);
      assert.ok(phases.every((phase, i) => i === 0 || phase >= phases[i - 1]));
      assert.ok(order.at(-1).closest('footer'));
      for (const element of order) {
        element.focus();
        assert.ok(document.activeElement === element, `cannot focus ${element.outerHTML}`);
        for (const shiftKey of [false, true]) {
          const key = new window.KeyboardEvent('keydown', { key: 'Tab', shiftKey, bubbles: true, cancelable: true });
          element.dispatchEvent(key);
          assert.equal(key.defaultPrevented, false, 'Tab and Shift+Tab must remain native');
        }
      }
    } finally { await close(); }
  });
}

// The stars: the build's reading is in the markup (lib/stars.ts); the page
// refreshes it, in the header and in the hero. These start from a slot of
// their own, so they hold whether or not the build that made dist/ could
// reach GitHub.
function starSlot(document, count) {
  const slot = document.querySelector('[data-stars]');
  slot.toggleAttribute('data-unknown', count === null);
  slot.querySelector('[data-count]').textContent = count === null ? '' : String(count);
  return slot;
}

test('the star count is refreshed from GitHub, and kept for an hour', async () => {
  const { window, document, run, close } = page();
  try {
    const slot = starSlot(document, null);
    window.localStorage.clear();
    let calls = 0;
    window.fetch = async (url) => {
      calls += 1;
      assert.equal(url, 'https://api.github.com/repos/mihaicristianfarcas/helio');
      return { ok: true, json: async () => ({ stargazers_count: 1234 }) };
    };
    run('components/Nav.astro');
    await new Promise(setImmediate);
    assert.equal(slot.hasAttribute('data-unknown'), false);
    assert.equal(slot.querySelector('[data-count]').textContent, '1.2k');
    assert.equal(document.querySelector('.hero [data-count]')?.textContent ?? '1.2k', '1.2k', 'the hero shows the same');
    assert.equal(document.querySelector('.gh').getAttribute('aria-label'), 'Source on GitHub, 1,234 stars');
    assert.equal(calls, 1);
    run('components/Nav.astro');
    await new Promise(setImmediate);
    assert.equal(calls, 1, 'a figure under an hour old is not asked for again');
  } finally { await close(); }
});

test('an older saved star count is shown at once, and GitHub is asked for the newer', async () => {
  const { window, document, run, close } = page();
  try {
    const slot = starSlot(document, 4);
    window.localStorage.setItem('helio:stars', JSON.stringify({ count: 999, at: Date.now() - 2 * 3_600_000 }));
    let answer;
    window.fetch = () => new Promise((resolve) => { answer = resolve; });
    run('components/Nav.astro');
    await new Promise(setImmediate);
    const text = () => slot.querySelector('[data-count]').textContent;
    assert.equal(text(), '999', 'the saved figure replaces the build\'s while GitHub is asked');
    answer({ ok: true, json: async () => ({ stargazers_count: 1300 }) });
    await new Promise(setImmediate);
    assert.equal(text(), '1.3k');
    assert.equal(JSON.parse(window.localStorage.getItem('helio:stars')).count, 1300);
  } finally { await close(); }
});

test('a refused, failed or malformed answer leaves the star count the page has', async () => {
  for (const answer of [
    async () => ({ ok: false, json: async () => ({}) }),
    async () => { throw new Error('offline'); },
    async () => ({ ok: true, json: async () => ({ stargazers_count: 'many' }) }),
    async () => ({ ok: true, json: async () => ({ stargazers_count: -1 }) }),
  ]) {
    for (const had of [7, null]) {
      const { window, document, run, close } = page();
      try {
        const slot = starSlot(document, had);
        window.localStorage.clear();
        window.fetch = answer;
        run('components/Nav.astro');
        await new Promise(setImmediate);
        assert.equal(slot.hasAttribute('data-unknown'), had === null);
        assert.equal(slot.querySelector('[data-count]').textContent, had === null ? '' : '7');
        assert.equal(window.localStorage.getItem('helio:stars'), null, 'only a good answer is kept');
      } finally { await close(); }
    }
  }
});

for (const [agent, system, button, listed] of [
  ['Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)', 'macos', 'Helio-macos-arm64', ['Helio-macos-x86_64', 'Helio-windows-x86_64-setup', 'Helio-linux-x86_64']],
  ['Mozilla/5.0 (Windows NT 10.0; Win64; x64)', 'windows', 'Helio-windows-x86_64-setup', ['Helio-macos-arm64', 'Helio-macos-x86_64', 'Helio-linux-x86_64']],
  ['Mozilla/5.0 (X11; Linux x86_64)', 'linux', 'Helio-linux-x86_64', ['Helio-macos-arm64', 'Helio-macos-x86_64', 'Helio-windows-x86_64-setup']],
]) {
  test(`on ${system} there is one download button, and a line of the other platforms under the steps`, async () => {
    const { window, document, run, stops, close } = page();
    try {
      Object.defineProperty(window.navigator, 'userAgent', { value: agent, configurable: true });
      run('layouts/Base.astro');
      run('components/Install.astro');
      const marks = [...document.querySelectorAll('.hero .button svg')].filter((mark) => window.getComputedStyle(mark).display !== 'none');
      assert.deepEqual(marks.map((mark) => mark.dataset.mark), [system], 'the hero\'s button wears the system\'s mark');
      const name = { macos: 'macOS', windows: 'Windows', linux: 'Linux' }[system];
      const label = [...document.querySelector('.hero .button').querySelectorAll('span:not(.mark)')]
        .filter((span) => window.getComputedStyle(span).display !== 'none' && !span.querySelector('span'))
        .map((span) => span.textContent).join('');
      assert.equal(label, ` for ${name}`, 'and says what it downloads for');
      const panels = [...document.querySelectorAll('[data-picker] .panel')].filter((panel) => !panel.hidden);
      assert.equal(panels.length, 1);
      assert.equal(panels[0].dataset.os, system);
      assert.deepEqual([...panels[0].querySelectorAll('.files [data-file]')].map((link) => link.dataset.file), [button]);
      assert.ok(panels[0].querySelector('.then + [data-others]'), 'the other platforms sit under the steps');
      const rows = [...document.querySelectorAll('[data-listed]')].filter((row) => !row.hidden);
      assert.deepEqual(rows.map((row) => row.dataset.listed), listed);
      assert.ok(rows.every((row) => stops().includes(row.querySelector('a'))));
    } finally { await close(); }
  });
}

const device = (window, agent, touch) => {
  Object.defineProperty(window.navigator, 'userAgent', { value: agent, configurable: true });
  Object.defineProperty(window.navigator, 'maxTouchPoints', { value: touch, configurable: true });
  const alerts = [];
  window.alert = (message) => alerts.push(message);
  return alerts;
};

for (const [name, agent] of [
  ['a phone', 'Mozilla/5.0 (iPhone; CPU iPhone OS 26_0 like Mac OS X)'],
  ['an Android tablet', 'Mozilla/5.0 (Linux; Android 15; Pixel Tablet)'],
  ['an iPad, which says it is a Mac', 'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)'],
]) {
  test(`${name} has no downloads, and each download button says why`, async () => {
    const { window, document, run, stops, close } = page();
    try {
      const alerts = device(window, agent, 5);
      run('layouts/Base.astro');
      run('components/Install.astro');
      assert.equal(stops().filter((stop) => stop.dataset?.file).length, 0);
      const links = [...document.querySelectorAll('a[href$="#install"]')];
      assert.ok(links.length >= 2, 'the header and the hero still offer the download');
      for (const link of links) link.click();
      assert.equal(alerts.length, links.length);
      assert.equal(alerts[0], 'Please open this page on a macOS, Windows or Linux device.');
    } finally { await close(); }
  });
}

test('a Mac, with no touch screen, keeps its downloads at any width', async () => {
  const { window, document, run, stops, close } = page([], 390);
  try {
    const alerts = device(window, 'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)', 0);
    run('layouts/Base.astro');
    assert.ok(stops().some((stop) => stop.dataset?.file));
    document.querySelector('header a[href$="#install"]').click();
    assert.deepEqual(alerts, []);
  } finally { await close(); }
});

test('section and skip links focus a visible action, never a heading or static region', async () => {
  const { document, enabled, close } = page(components);
  try {
    const links = [...document.querySelectorAll('header a[href^="#"], a.skip')];
    assert.ok(links.length >= 3);
    for (const link of links) {
      link.focus();
      link.click();
      const target = document.getElementById(link.getAttribute('href').slice(1));
      const first = [...target.querySelectorAll(actions)].find(enabled);
      assert.ok(document.activeElement === first, `expected an action inside ${target.id}`);
      assert.ok(document.activeElement.matches(actions));
    }
    assert.equal(document.querySelectorAll('h1[tabindex], h2[tabindex], main[tabindex], section[tabindex]').length, 0);
  } finally { await close(); }
});

test('missing downloads leave focus on an available link and cannot be followed', async () => {
  const { window, document, run, stops, close } = page(['layouts/Base.astro']);
  try {
    let respond;
    window.fetch = () => new Promise((resolve) => { respond = resolve; });
    run('components/Install.astro');
    const download = document.querySelector('[data-picker] .panel:not([hidden]) [data-file]');
    download.focus();
    respond({ ok: true, json: async () => ({ version: '1.0.0', commit: 'abcdef123456', published: '2026-10-04', files: {} }) });
    await new Promise(setImmediate);
    assert.equal(download.hasAttribute('href'), false);
    assert.ok(!stops().includes(download));
    assert.ok(document.activeElement === document.querySelector('[data-picker] .build > a'));
  } finally { await close(); }
});

test('without JavaScript static content has no Tab stops, and every download stays available', async () => {
  const { document, stops, close } = page();
  try {
    assert.ok(stops().every((element) => element.matches(actions)));
    assert.equal(document.querySelectorAll('[data-picker] .panel[hidden]').length, 0);
    assert.ok(stops().filter((element) => element.matches('[data-file]')).length >= 6);
  } finally { await close(); }
});

test('the download card is built with its release, and that release arriving changes nothing', async () => {
  const { window, document, run, close } = page(['layouts/Base.astro']);
  try {
    const picker = document.querySelector('[data-picker]');
    const commit = picker.dataset.commit;
    if (!commit) {
      // Built without network: the card says what it can, and the page completes it.
      assert.equal(picker.querySelector('[data-build]').textContent.trim(), 'The newest released version.');
      return;
    }
    // As a browser shows it: runs of white space are one space.
    const shown = (element) => element.textContent.replace(/\s+/g, ' ').trim();
    assert.match(shown(picker.querySelector('[data-build]')), /^Version \S+, built from [0-9a-f]{7} on \w+ \d+, \d{4}\.$/);
    for (const size of picker.querySelectorAll('[data-size]')) assert.match(shown(size), /^(\d+\.\d MB|not built yet)$/);
    let respond;
    window.fetch = () => new Promise((resolve) => { respond = resolve; });
    run('components/Install.astro');
    const before = picker.innerHTML;
    // The same commit with no files: were it read, every link would close.
    respond({ ok: true, json: async () => ({ version: '9.9.9', commit, published: '2030-01-02T00:00:00Z', files: {} }) });
    await new Promise(setImmediate);
    assert.equal(picker.innerHTML, before);
  } finally { await close(); }
});

test('a release newer than the build replaces the card\'s version, sizes and links', async () => {
  const { window, document, run, close } = page(['layouts/Base.astro']);
  try {
    const picker = document.querySelector('[data-picker]');
    let respond;
    window.fetch = () => new Promise((resolve) => { respond = resolve; });
    run('components/Install.astro');
    const ids = ['Helio-macos-arm64', 'Helio-macos-x86_64', 'Helio-windows-x86_64-setup', 'Helio-linux-x86_64'];
    const files = Object.fromEntries(ids.map((id) => [id, { url: `https://example.org/${id}`, size: 12_345_678 }]));
    respond({ ok: true, json: async () => ({ version: '9.9.9', commit: 'f'.repeat(40), published: '2030-01-02T00:00:00Z', files }) });
    await new Promise(setImmediate);
    assert.equal(picker.querySelector('[data-build]').textContent, 'Version 9.9.9, built from fffffff on January 2, 2030.');
    for (const link of picker.querySelectorAll('[data-file]')) {
      assert.equal(link.getAttribute('href'), `https://example.org/${link.dataset.file}?fffffff`);
      assert.equal(link.hasAttribute('aria-disabled'), false);
    }
    for (const size of picker.querySelectorAll('[data-size]')) assert.equal(size.textContent, '12.3 MB');
  } finally { await close(); }
});

test('each of the landing page\'s own links finds its section there', () => {
  const ids = new Set([...html.matchAll(/\sid="([^"]+)"/g)].map((match) => match[1]));
  const links = [...html.matchAll(/href="#([^"]*)"/g)].map((match) => match[1]);
  for (const section of ['top', 'install', 'agents', 'main']) assert.ok(links.includes(section), section);
  for (const link of links) assert.ok(ids.has(link), `#${link}`);
});

// The documentation: the repository's Markdown, built into dist/docs/.
const built = (path) => {
  const file = join(site, 'dist', path.replace(/^\//, ''), 'index.html');
  return existsSync(file) ? readFileSync(file, 'utf8') : null;
};
const docs = () => readdirSync(join(site, 'dist/docs'), { recursive: true })
  .filter((file) => file.endsWith('index.html'))
  .map((file) => `/docs/${file.replace(/index\.html$/, '')}`.replace(/\/+$/, '/'));

test('every page of the documentation is built, and listed beside every other', () => {
  const pages = docs().sort();
  assert.equal(pages.length, 19);
  assert.ok(pages.includes('/docs/') && pages.includes('/docs/platforms/windows/') && pages.includes('/docs/agents/'));
  const sitemap = readFileSync(join(site, 'dist/sitemap.xml'), 'utf8');
  for (const page of pages) {
    const html = built(page);
    const sidebar = [...html.matchAll(/<aside class="pages"[\s\S]*?<\/aside>/g)][0][0];
    assert.deepEqual([...sidebar.matchAll(/href="(\/docs\/[^"]*)"/g)].map((m) => m[1]).sort(), pages, page);
    assert.match(sidebar, new RegExp(`href="${page}"[^>]*aria-current="page"`), `${page} is marked as the one open`);
    assert.ok(sitemap.includes(`${page}</loc>`), `${page} is in the sitemap`);
  }
});

test('the Overview starts on its text, without the link preview the hero already shows', () => {
  const readme = readFileSync(join(dirname(site), 'README.md'), 'utf8');
  assert.ok(readme.includes('](docs/images/helio.jpg)'), 'README.md opens on the link preview');
  const overview = built('/docs/').match(/<article class="prose"[^>]*>([\s\S]*?)<\/article>/)[1];
  const pictures = [...overview.matchAll(/<img [^>]*alt="([^"]*)"/g)].map((m) => m[1]);
  assert.ok(pictures.length > 0, 'the Overview keeps its other pictures');
  assert.ok(!pictures.some((alt) => alt.startsWith('Helio: low-latency')), pictures.join('\n'));
  assert.ok(!/<p>\s*<\/p>/.test(overview), 'no empty paragraph is left where it was');
});

test('every Markdown file in docs/ is a page of the documentation', async () => {
  const { PAGES } = await import('../src/lib/docs.mjs');
  const repo = dirname(site);
  const files = readdirSync(join(repo, 'docs'), { recursive: true })
    .filter((file) => file.endsWith('.md') && file !== 'README.md')
    .map((file) => `docs/${file.split('\\').join('/')}`);
  for (const file of files) assert.ok(PAGES.some((page) => page.file === file), `${file} is missing from src/lib/docs.mjs`);
  for (const file of ['README.md', 'AGENTS.md', 'CONTRIBUTING.md', 'SECURITY.md']) {
    assert.ok(PAGES.some((page) => page.file === file), file);
  }
});

test('every link in the documentation leads to a page and a section that exist', () => {
  for (const page of docs()) {
    const html = built(page);
    const article = html.match(/<article class="prose"[^>]*>([\s\S]*?)<\/article>/)[1];
    for (const [, href] of article.matchAll(/<a [^>]*href="([^"]*)"/g)) {
      if (/^[a-z]+:/i.test(href)) continue;
      assert.doesNotMatch(href, /\.md(#|$)/, `${page}: ${href} is still a link to Markdown`);
      const [path, anchor] = href.split('#');
      const target = path ? built(path) : html;
      assert.ok(target, `${page}: ${href} leads to no page`);
      if (anchor) assert.ok(target.includes(`id="${anchor}"`), `${page}: ${href} leads to no section`);
    }
  }
});

test('the site does not name the apps as they were called before', () => {
  for (const file of ['dist/index.html', 'dist/robots.txt', 'dist/sitemap.xml']) {
    assert.doesNotMatch(readFileSync(join(site, file), 'utf8'), /pingpong|\bPing\b|\bPong\b/i);
  }
});

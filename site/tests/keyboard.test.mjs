//! Keyboard regressions against the built page and its actual component scripts.
//! DOM checks cover control order and focus changes; they do not render a browser.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
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

const components = [
  'components/Nav.astro', 'components/Gallery.astro', 'components/Install.astro',
  'components/Command.astro', 'components/Video.astro', 'layouts/Base.astro',
];

test('cached fonts cannot restore a refresh before the gallery and downloads initialize', async () => {
  const { window, document, run, close } = page();
  try {
    document.fonts = { ready: Promise.resolve() };
    window.performance.getEntriesByType = () => [{ type: 'reload' }];
    window.sessionStorage.setItem('scroll:/', '400');
    run('layouts/Base.astro');
    await Promise.resolve();
    assert.equal(window.scrollY, 0, 'the initial expanded layout must not receive the saved position');
    assert.equal(document.documentElement.dataset.restore, '400');
    for (const component of components.filter((component) => component !== 'layouts/Base.astro')) run(component);
    assert.equal(document.querySelector('[data-gallery]').hasAttribute('data-ready'), true);
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
    assert.equal(header.hasAttribute('data-hidden'), false);
    scrollTo(0);
    assert.equal(header.hasAttribute('data-stuck'), false);
  } finally { await close(); }
});

for (const width of [1280, 390]) {
  test(`at ${width}px the page offers every visible action, in header-to-footer order`, async () => {
    const { window, document, enabled, stops, close } = page(components, width);
    try {
      if (width < 820) document.querySelector('[data-menu]').click();
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

test('pressing a page stops the pages changing, though Safari gives a clicked button no focus', async () => {
  const { window, document, close } = page(components);
  try {
    const gallery = document.querySelector('[data-gallery]');
    assert.equal(gallery.hasAttribute('data-auto'), true, 'the pages start by themselves');
    assert.equal(gallery.querySelector('[data-toggle]'), null);
    const choices = [...gallery.querySelectorAll('[data-tab]')];
    choices[2].click();
    assert.equal(gallery.hasAttribute('data-auto'), false);
    const fill = new window.Event('animationend', { bubbles: true });
    fill.animationName = 'fill';
    gallery.dispatchEvent(fill);
    assert.equal(choices[2].getAttribute('aria-pressed'), 'true', 'the chosen page stays');
  } finally { await close(); }
});

test('every gallery choice remains a button in the Tab order and focus stops autoplay', async () => {
  const { document, stops, close } = page(components);
  try {
    const gallery = document.querySelector('[data-gallery]');
    const choices = [...gallery.querySelectorAll('[data-tab]')];
    assert.equal(choices.length, 5);
    assert.ok(choices.every((button) => stops().includes(button)));
    for (const [i, button] of choices.entries()) {
      button.focus();
      assert.equal(gallery.hasAttribute('data-auto'), false);
      button.click();
      assert.ok(document.activeElement === button);
      assert.equal(button.getAttribute('aria-pressed'), 'true');
      assert.equal(gallery.querySelector(`[data-pane="${i}"]`).getAttribute('aria-hidden'), 'false');
      assert.ok(choices.every((choice) => stops().includes(choice)));
    }
  } finally { await close(); }
});

for (const [agent, system, button, listed] of [
  ['Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)', 'macos', 'Helio-macos-arm64', ['Helio-macos-x86_64', 'Helio-windows-x86_64-setup', 'Helio-linux-x86_64']],
  ['Mozilla/5.0 (Windows NT 10.0; Win64; x64)', 'windows', 'Helio-windows-x86_64-setup', ['Helio-macos-arm64', 'Helio-macos-x86_64', 'Helio-linux-x86_64']],
  ['Mozilla/5.0 (X11; Linux x86_64)', 'linux', 'Helio-linux-x86_64', ['Helio-macos-arm64', 'Helio-macos-x86_64', 'Helio-windows-x86_64-setup']],
]) {
  test(`on ${system} there is one download button, and a line of the other platforms under the steps`, async () => {
    const { window, document, run, stops, close } = page(['layouts/Base.astro']);
    try {
      Object.defineProperty(window.navigator, 'userAgent', { value: agent, configurable: true });
      run('components/Install.astro');
      const panels = [...document.querySelectorAll('[data-picker] .panel')].filter((panel) => !panel.hidden);
      assert.equal(panels.length, 1);
      assert.equal(panels[0].dataset.os, system);
      assert.deepEqual([...panels[0].querySelectorAll('.files [data-file]')].map((link) => link.dataset.file), [button]);
      assert.ok(panels[0].querySelector('.then + [data-others]'), 'the other platforms sit under the steps');
      const rows = [...document.querySelectorAll('[data-listed]')].filter((row) => !row.hidden);
      assert.deepEqual(rows.map((row) => row.dataset.listed), listed);
      assert.ok(rows.every((row) => stops().includes(row.querySelector('a'))));
      assert.equal(document.querySelectorAll('[data-select-app], [data-select-os]').length, 0);
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
      assert.equal(stops().filter((stop) => stop.dataset?.file).length, 0);
      const links = [...document.querySelectorAll('a[href$="#install"]')];
      assert.ok(links.length >= 2);
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
    document.querySelector('a[href$="#install"]').click();
    assert.deepEqual(alerts, []);
  } finally { await close(); }
});

test('section and skip links focus a visible action, never a heading or static region', async () => {
  const { document, enabled, close } = page(components);
  try {
    const links = [...document.querySelectorAll('header a[href^="#"], a.skip')];
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

test('closed FAQ answers and inactive download panels never enter the Tab order', async () => {
  const { window, document, stops, close } = page(components);
  try {
    const details = document.querySelector('details');
    const answerLink = details.querySelector('p a');
    assert.ok(!stops().includes(answerLink));
    details.open = true;
    assert.ok(stops().includes(answerLink));
    details.open = false;
    assert.ok(!stops().includes(answerLink));
    const hiddenPanels = document.querySelectorAll('[data-picker] .panel[hidden]');
    assert.equal(hiddenPanels.length, 2);
    for (const panel of hiddenPanels) {
      assert.equal(window.getComputedStyle(panel).display, 'none');
      assert.ok(stops().every((element) => !panel.contains(element)));
    }
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
    respond({ ok: true, json: async () => ({ version: '1.0', commit: 'abcdef123456', published: '2026-10-04', files: {} }) });
    await new Promise(setImmediate);
    assert.equal(download.hasAttribute('href'), false);
    assert.ok(!stops().includes(download));
    assert.ok(document.activeElement === document.querySelector('[data-picker] .build > a'));
  } finally { await close(); }
});

test('without JavaScript static content has no Tab stops and every download stays available', async () => {
  const { document, stops, close } = page();
  try {
    assert.ok(stops().every((element) => element.matches(actions)));
    assert.equal(document.querySelectorAll('[data-picker] .panel[hidden]').length, 0);
    assert.ok(stops().filter((element) => element.matches('[data-file]')).length >= 6);
  } finally { await close(); }
});

test('the hero badge is the one place the site says the version', () => {
  const pages = ['dist/index.html', 'dist/agents/index.html'].map((file) => readFileSync(join(site, file), 'utf8'));
  const text = pages.map((page) => page.replace(/<script\b[^>]*>[\s\S]*?<\/script>/gi, '').replace(/<[^>]+>/g, ' '));
  assert.equal(text.join(' ').match(/Version\s+\d/g)?.length ?? 0, 1);
  assert.match(pages[0], /class="badge"[^>]*>Version \d/);
});

test('the agents page reaches the home page from its header and footer', () => {
  const agents = readFileSync(join(site, 'dist/agents/index.html'), 'utf8');
  for (const section of ['features', 'latency', 'platforms', 'faq', 'install']) {
    assert.match(agents, new RegExp(`href="/#${section}"`));
  }
  assert.doesNotMatch(agents.replace(/<script\b[^>]*>[\s\S]*?<\/script>/gi, ''), /href="#(?!main)/);
});

test('neither page names the apps as they were called before', () => {
  for (const file of ['dist/index.html', 'dist/agents/index.html', 'dist/robots.txt', 'dist/sitemap.xml']) {
    assert.doesNotMatch(readFileSync(join(site, file), 'utf8'), /pingpong|\bPing\b|\bPong\b/i);
  }
});

//! The repository's star count, for the GitHub button in the header. The build
//! reads it once, so the figure is in the HTML from the first paint and the
//! nav links beside it do not move when a number arrives later; the page
//! refreshes it in the browser (components/Nav.astro). A build with no network,
//! or GitHub refusing it, leaves the button as an icon alone: this never fails
//! the build.
import { REPO_API } from './repo';

/** Long enough for GitHub on a slow runner, short enough not to stall a build offline. */
const TIMEOUT_MS = 5000;

let read: Promise<number | null> | undefined;

/** The stars now, or null when GitHub could not say. Read once per build. */
export function stars(): Promise<number | null> {
  read ??= fetchStars();
  return read;
}

async function fetchStars(): Promise<number | null> {
  // The workflow passes its token: GitHub's runners share addresses, and an
  // anonymous call from one is often over its hourly limit.
  const token = process.env.GITHUB_TOKEN;
  try {
    const response = await fetch(REPO_API, {
      headers: { accept: 'application/vnd.github+json', ...(token && { authorization: `Bearer ${token}` }) },
      signal: AbortSignal.timeout(TIMEOUT_MS),
    });
    if (!response.ok) return null;
    const count = ((await response.json()) as { stargazers_count?: unknown }).stargazers_count;
    return Number.isSafeInteger(count) && (count as number) >= 0 ? (count as number) : null;
  } catch {
    return null;
  }
}

/** 987, 1.2k, 12k: how GitHub shortens its own counters. */
export const short = (count: number) =>
  new Intl.NumberFormat('en', { notation: 'compact', maximumFractionDigits: 1 }).format(count).toLowerCase();

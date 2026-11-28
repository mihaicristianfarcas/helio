//! The release the downloads are (latest.json), for the download card. The
//! build reads it once, so the card has its version, its sizes and its links
//! in the HTML from the first paint and nothing in it moves as the page loads;
//! the page reads it again in the browser and changes the card only for a
//! newer release (components/Install.astro). A build with no network, or an
//! answer that is not a release, leaves the card to the browser, as stars.ts
//! does: this never fails the build.
import { MANIFEST } from './downloads';

export interface Release {
  version: string;
  commit: string;
  /** When it was published, ISO 8601. */
  published: string;
  /** Each download by its key (downloads.ts, File.id): where it is, and its size in bytes. */
  files: Record<string, { url: string; size: number }>;
}

/** As stars.ts waits: long enough for a slow runner, short enough not to stall a build offline. */
const TIMEOUT_MS = 5000;

let read: Promise<Release | null> | undefined;

/** The release now, or null when latest.json could not be read. Read once per build. */
export function release(): Promise<Release | null> {
  read ??= fetchRelease();
  return read;
}

async function fetchRelease(): Promise<Release | null> {
  try {
    const response = await fetch(MANIFEST, { signal: AbortSignal.timeout(TIMEOUT_MS) });
    if (!response.ok) return null;
    const m = (await response.json()) as Partial<Release>;
    const whole =
      typeof m.version === 'string' &&
      typeof m.commit === 'string' &&
      /^[0-9a-f]{7,40}$/.test(m.commit) &&
      typeof m.published === 'string' &&
      !Number.isNaN(Date.parse(m.published)) &&
      typeof m.files === 'object' &&
      m.files !== null &&
      Object.values(m.files).every((f) => typeof f?.url === 'string' && Number.isFinite(f?.size));
    return whole ? (m as Release) : null;
  } catch {
    return null;
  }
}

/** 13.1 MB, as the card's script writes a newer release's sizes. */
export const megabytes = (bytes: number) => `${(bytes / 1e6).toFixed(1)} MB`;

/** November 5, 2026: the day in UTC, so the build and every browser name the same one. */
export const day = (iso: string) => new Date(iso).toLocaleDateString('en', { dateStyle: 'long', timeZone: 'UTC' });

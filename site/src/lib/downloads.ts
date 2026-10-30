// The downloads: the files the release workflow publishes for each push to
// main (tools/publish-downloads), on Cloudflare R2. Their names stay the
// same from one release to the next, so the page links them as they are;
// latest.json says which build they are, and the page reads it as it loads.

export const DOWNLOADS = 'https://download.helio.mihaicristian.dev';

/** What the files are: version, commit, and each one's size and SHA-256. */
export const MANIFEST = `${DOWNLOADS}/latest.json`;

export const CHECKSUMS = `${DOWNLOADS}/latest/SHA256SUMS`;

export type System = 'macos' | 'windows' | 'linux';

export interface File {
  /** Its key in latest.json's files. */
  id: string;
  url: string;
  /** What the line of other platforms calls it. */
  label: string;
  system: System;
}

const file = (id: string, ext: string, label: string, system: System): File => ({
  id,
  url: `${DOWNLOADS}/latest/${id}${ext}`,
  label,
  system,
});

/**
 * Every download, as docs/install.md lists them. A browser does not say
 * which Mac it is on, so Apple silicon, the larger share, is the one the
 * button offers and Intel is in the line of other platforms.
 */
export const FILES = {
  macArm: file('Helio-macos-arm64', '.dmg', 'macOS (Apple silicon)', 'macos'),
  macIntel: file('Helio-macos-x86_64', '.dmg', 'macOS (Intel)', 'macos'),
  windows: file('Helio-windows-x86_64-setup', '.exe', 'Windows', 'windows'),
  linux: file('Helio-linux-x86_64', '.tar.gz', 'Linux', 'linux'),
};

// Where the page's links lead: the repository on GitHub, and the release
// the page describes.
import cargo from '../../../Cargo.toml?raw';

export const REPO = 'https://github.com/mihaicristianfarcas/helio';

/**
 * The workspace's version (Cargo.toml). The site goes live only from a
 * release's tag, over which it is rebased, so this is the version it
 * describes.
 */
export const VERSION = (() => {
  const version = /^\[workspace\.package\]$[\s\S]*?^version = "([^"]+)"$/m.exec(cargo)?.[1];
  if (!version) throw new Error('Cargo.toml has no [workspace.package] version');
  return version;
})();

/** That version's release on GitHub: its notes are .github/release-notes/VERSION.md. */
export const RELEASE_NOTES = `${REPO}/releases/tag/v${VERSION}`;

/** The same repository in GitHub's API, which says its star count. */
export const REPO_API = REPO.replace('https://github.com/', 'https://api.github.com/repos/');

/** A file in the repository, on GitHub. */
export const onGitHub = (path: string) => `${REPO}/blob/main/${path}`;

/** The form that asks a tester how it went (.github/ISSUE_TEMPLATE). */
export const TESTER_REPORT = `${REPO}/issues/new?template=tester-report.yml`;

export const DISCUSSIONS = `${REPO}/discussions`;

/** README.md's list of what Helio does not do. */
export const LIMITATIONS = `${REPO}#known-limitations`;

export const TAP = `brew tap mihaicristianfarcas/helio ${REPO}`;

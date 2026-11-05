// Where the page's facts come from: the repository on GitHub, and files in
// this checkout that change with each release.
import cask from '../../../Casks/helio.rb?raw';

export const REPO = 'https://github.com/mihaicristianfarcas/helio';

/** A file in the repository, on GitHub. */
export const onGitHub = (path: string) => `${REPO}/blob/main/${path}`;

/** The form that asks a tester how it went (.github/ISSUE_TEMPLATE). */
export const TESTER_REPORT = `${REPO}/issues/new?template=tester-report.yml`;

export const DISCUSSIONS = `${REPO}/discussions`;

/** README.md's list of what Helio does not do. */
export const LIMITATIONS = `${REPO}#known-limitations`;

/**
 * The release the Homebrew cask installs. tools/update-casks rewrites the
 * cask at each release, so the page shows the version people get. The
 * hero's badge is the one place the page says it.
 */
export const VERSION = /^\s*version "([^"]+)"/m.exec(cask)?.[1] ?? null;

export const TAP = `brew tap mihaicristianfarcas/helio ${REPO}`;

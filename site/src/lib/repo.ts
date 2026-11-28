// Where the page's links lead: the repository on GitHub.

export const REPO = 'https://github.com/mihaicristianfarcas/helio';

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

/** The cask by its full name: Homebrew's own casks have a music composition
 *  app named `helio`, which the short name installs, and the full name also
 *  trusts this one cask, which Homebrew 6 and later ask of another's tap
 *  (docs/install.md). */
export const CASK_INSTALL = 'brew install --cask mihaicristianfarcas/helio/helio';

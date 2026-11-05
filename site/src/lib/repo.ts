// Where the page's links lead: the repository on GitHub.

export const REPO = 'https://github.com/mihaicristianfarcas/helio';

/** A file in the repository, on GitHub. */
export const onGitHub = (path: string) => `${REPO}/blob/main/${path}`;

/** The form that asks a tester how it went (.github/ISSUE_TEMPLATE). */
export const TESTER_REPORT = `${REPO}/issues/new?template=tester-report.yml`;

export const DISCUSSIONS = `${REPO}/discussions`;

/** README.md's list of what Helio does not do. */
export const LIMITATIONS = `${REPO}#known-limitations`;

export const TAP = `brew tap mihaicristianfarcas/helio ${REPO}`;

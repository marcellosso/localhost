export const repository = 'https://github.com/marcellosso/localhost';
export const releases = `${repository}/releases/latest`;
/** The macOS app from the Homebrew tap, the same app as the DMG. */
export const brewInstall = 'brew install --cask marcellosso/tap/localhost';

export const name = 'localhost app';
export const tagline = 'Every local project, one place to run it.';

/** A path inside the site, without a trailing slash (the URLs the site is served at). */
export function href(path: string): string {
  const base = import.meta.env.BASE_URL.replace(/\/$/, '');
  return `${base}/${path.replace(/^\//, '')}`.replace(/(.)\/$/, '$1');
}

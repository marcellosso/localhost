export const repository = 'https://github.com/marcellosso/localhost';
export const releases = `${repository}/releases/latest`;

export const name = 'localhost';
export const tagline = 'Every local project, one place to run it.';

/** A path inside the site, without a trailing slash (the URLs the site is served at). */
export function href(path: string): string {
  const base = import.meta.env.BASE_URL.replace(/\/$/, '');
  return `${base}/${path.replace(/^\//, '')}`.replace(/(.)\/$/, '$1');
}

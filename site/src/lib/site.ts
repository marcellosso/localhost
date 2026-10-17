export const repository = 'https://github.com/marcellosso/localhost';
export const releases = `${repository}/releases/latest`;

/** A path inside the site, under the base it is deployed at. */
export function href(path: string): string {
  const base = import.meta.env.BASE_URL.replace(/\/$/, '');
  return `${base}/${path.replace(/^\//, '')}`;
}

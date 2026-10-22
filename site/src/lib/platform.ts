import type { Platform } from './release';

export const names: Record<Platform, string> = { macos: 'macOS', windows: 'Windows', linux: 'Linux' };

export function detectPlatform(): Platform {
  const platform = (navigator as Navigator & { userAgentData?: { platform: string } }).userAgentData?.platform ?? navigator.userAgent;
  if (/win/i.test(platform)) return 'windows';
  if (/linux|x11|cros/i.test(platform) && !/android/i.test(platform)) return 'linux';
  return 'macos';
}

/** Opens the install dialog (InstallDialog.astro) on a platform's steps. */
export function openInstall(platform: Platform, started: boolean) {
  document.dispatchEvent(new CustomEvent('install:open', { detail: { platform, started } }));
}

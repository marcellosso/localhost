import { detectPlatform } from './platform';
import { href } from './site';

/** Behaviour shared by the landing prototypes (pages/prototypes/), which draw their own header. */
export function startPrototype() {
  const platform = detectPlatform();
  document.documentElement.dataset.platform = platform;

  // localhost is a desktop app: a phone gets the install guide, not a DMG it can't open.
  // detectPlatform() reads an iPhone as macOS and Android as neither, so phones are told apart here.
  const phone = /iPhone|iPod|Android.+Mobile/i.test(navigator.userAgent);
  if (phone) {
    document.documentElement.dataset.device = 'phone';
    // DownloadButton labels itself in its own script; this runs after it.
    setTimeout(() => {
      for (const button of document.querySelectorAll<HTMLAnchorElement>('[data-download]')) {
        button.href = href('docs/install');
        button.dataset.trackPlatform = 'phone';
        const label = button.querySelector<HTMLElement>('[data-download-label="large"]');
        if (label) label.textContent = 'Get it for your computer';
      }
    });
    // Capturing at the document stops DownloadButton's own click handler, which would open the dialog.
    document.addEventListener(
      'click',
      (event) => {
        const button = (event.target as Element | null)?.closest<HTMLAnchorElement>('[data-download]');
        if (!button) return;
        event.stopPropagation();
        event.preventDefault();
        location.href = href('docs/install');
      },
      { capture: true },
    );
  }

  for (const button of document.querySelectorAll<HTMLButtonElement>('[data-copy-text]')) {
    const label = button.textContent;
    button.addEventListener('click', () => {
      navigator.clipboard.writeText(button.dataset.copyText!).then(() => {
        button.textContent = 'Copied';
        setTimeout(() => (button.textContent = label), 1500);
      });
    });
  }

  const toggle = document.querySelector<HTMLButtonElement>('[data-theme-toggle]');
  if (toggle) {
    const label = () => {
      const dark = document.documentElement.dataset.theme === 'dark';
      toggle.setAttribute('aria-label', dark ? 'Switch to light appearance' : 'Switch to dark appearance');
    };
    label();
    toggle.addEventListener('click', () => {
      const next = document.documentElement.dataset.theme === 'dark' ? 'light' : 'dark';
      document.documentElement.dataset.theme = next;
      localStorage.setItem('theme', next);
      label();
    });
  }
}

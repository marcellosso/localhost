import { detectPlatform, isPhone } from './platform';

/** Behaviour every page shares: the visitor's platform for CSS (`.mac-only`), and copy buttons. */
export function startPage() {
  document.documentElement.dataset.platform = detectPlatform();
  if (isPhone()) document.documentElement.dataset.device = 'phone';

  for (const button of document.querySelectorAll<HTMLButtonElement>('[data-copy-text]')) {
    const label = button.textContent;
    button.addEventListener('click', () => {
      navigator.clipboard.writeText(button.dataset.copyText!).then(() => {
        button.textContent = 'Copied';
        setTimeout(() => (button.textContent = label), 1500);
      });
    });
  }
}

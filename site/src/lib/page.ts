/** Behaviour every page shares: copy buttons. The visitor's platform for CSS is set in Base.astro's head. */
export function startPage() {
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

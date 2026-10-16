import { isTextField } from "./keyboard";

/**
 * Whether a right click keeps the webview's own menu: in a field or on
 * selected text, for Cut, Copy and Paste. Elsewhere it would offer Reload,
 * Back, Print and the like (WebView2 on Windows, WebKitGTK on Linux), which
 * an app doesn't have. The app's own context menus have taken the event already.
 */
export function keepsWebviewMenu(target: EventTarget | null, selection: Selection | null): boolean {
  return isTextField(target instanceof Element ? target : null) || (selection !== null && !selection.isCollapsed);
}

/** Turns the webview's menu off outside text, except in development (Inspect Element). */
export function guardContextMenu() {
  if (import.meta.env.DEV) return;
  window.addEventListener("contextmenu", (event) => {
    if (!event.defaultPrevented && !keepsWebviewMenu(event.target, document.getSelection())) event.preventDefault();
  });
}

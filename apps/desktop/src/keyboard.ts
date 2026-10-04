// Rules for when a window-level shortcut may act on a key press.

/** Controls that take typed input themselves; shortcuts must not steal their keys. */
export function isEditableTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) {
    return false;
  }
  if (target.isContentEditable) {
    return true;
  }
  return (
    target instanceof HTMLInputElement ||
    target instanceof HTMLTextAreaElement ||
    target instanceof HTMLSelectElement
  );
}

/** Elements the browser activates with Space or Enter. A global Space shortcut must leave them
 *  alone, or a button focused with Tab could never be pressed from the keyboard. */
export function isActivatableTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) {
    return false;
  }
  return (
    target instanceof HTMLButtonElement ||
    target instanceof HTMLAnchorElement ||
    target.tagName === "SUMMARY" ||
    target.getAttribute("role") === "button" ||
    target.getAttribute("role") === "tab" ||
    target.getAttribute("role") === "menuitem"
  );
}

/** True when a global shortcut should do nothing for this event. */
export function shouldIgnoreShortcut(e: KeyboardEvent, openDialogs: number): boolean {
  return openDialogs > 0 || isEditableTarget(e.target);
}

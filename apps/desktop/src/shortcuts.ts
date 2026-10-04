// Key bindings: how a binding is written, matched against a keyboard event, and shown to people.

export interface KeySpec {
  /** `KeyboardEvent.key`, compared case-insensitively. */
  key?: string;
  /** `KeyboardEvent.code`, for keys whose `key` varies with the layout (Space). */
  code?: string;
  ctrl?: boolean;
  shift?: boolean;
  alt?: boolean;
  /** Match with or without Shift (letters that only care about the key itself). */
  anyShift?: boolean;
}

/** True when the event is this binding. Ctrl and Meta both count as Ctrl. */
export function matchesKeys(e: KeyboardEvent, spec: KeySpec): boolean {
  if (Boolean(spec.ctrl) !== (e.ctrlKey || e.metaKey)) {
    return false;
  }
  if (Boolean(spec.alt) !== e.altKey) {
    return false;
  }
  if (!spec.anyShift && Boolean(spec.shift) !== e.shiftKey) {
    return false;
  }
  if (spec.code !== undefined) {
    return e.code === spec.code;
  }
  return spec.key !== undefined && e.key.toLowerCase() === spec.key.toLowerCase();
}

const KEY_NAMES: Record<string, string> = {
  ArrowLeft: "←",
  ArrowRight: "→",
  ArrowUp: "↑",
  ArrowDown: "↓",
  Escape: "Esc",
  Delete: "Del",
  Backspace: "⌫",
  " ": "Space",
};

/** Human-readable form, e.g. `Ctrl+Shift+Z`. */
export function formatKeys(spec: KeySpec): string {
  const parts: string[] = [];
  if (spec.ctrl) {
    parts.push("Ctrl");
  }
  if (spec.alt) {
    parts.push("Alt");
  }
  if (spec.shift) {
    parts.push("Shift");
  }
  const base = spec.code === "Space" ? "Space" : (spec.key ?? "");
  const named = KEY_NAMES[base] ?? base;
  parts.push(named.length === 1 ? named.toUpperCase() : named);
  return parts.join("+");
}

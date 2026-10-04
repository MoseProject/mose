// Editable keyboard shortcuts: how a binding is stored, parsed, recorded from a key press,
// validated and checked for conflicts, plus the store of the user's changes and its persistence.
// It knows nothing about the command table (commands.ts imports it), so it works on plain
// `{ id, keys }` rows.

import { api } from "./ipc";
import type { TextKey } from "./i18n";
import { formatKeys, type KeySpec } from "./shortcuts";
import { createStore, run } from "./store";

/** What a command offers by default and under which id. */
export interface Bindable {
  id: string;
  keys?: KeySpec[];
}

/** The user's changes: command id to its bindings in stored form. A command that is absent
 *  keeps its defaults; an empty list means the user unbound it. */
export type Overrides = Record<string, string[]>;

export const MAX_BINDINGS_PER_COMMAND = 4;

const MODIFIERS = ["Ctrl+", "Alt+", "Shift?+", "Shift+"] as const;

/** Stored form, e.g. `Ctrl+Shift+Z`, `Space`, `Shift?+S` (Shift optional). */
export function serializeKeys(spec: KeySpec): string {
  const parts: string[] = [];
  if (spec.ctrl) {
    parts.push("Ctrl");
  }
  if (spec.alt) {
    parts.push("Alt");
  }
  if (spec.anyShift) {
    parts.push("Shift?");
  } else if (spec.shift) {
    parts.push("Shift");
  }
  const key = spec.code === "Space" ? "Space" : (spec.key ?? "");
  parts.push(key.length === 1 ? key.toUpperCase() : key);
  return parts.join("+");
}

/** Inverse of [`serializeKeys`]; `null` for anything that is not a well-formed binding. */
export function parseKeys(text: string): KeySpec | null {
  let rest = text.trim();
  const spec: KeySpec = {};
  let consumed = true;
  while (consumed) {
    consumed = false;
    for (const modifier of MODIFIERS) {
      if (rest.toLowerCase().startsWith(modifier.toLowerCase()) && rest.length > modifier.length) {
        if (modifier === "Ctrl+") {
          spec.ctrl = true;
        } else if (modifier === "Alt+") {
          spec.alt = true;
        } else if (modifier === "Shift+") {
          spec.shift = true;
        } else {
          spec.anyShift = true;
        }
        rest = rest.slice(modifier.length);
        consumed = true;
        break;
      }
    }
  }
  if (
    rest === "" ||
    rest.includes("\u0000") ||
    rest.length > 24 ||
    (rest.length > 1 && rest.includes("+"))
  ) {
    return null;
  }
  if (rest.toLowerCase() === "space") {
    spec.code = "Space";
  } else {
    spec.key = rest.length === 1 ? rest.toLowerCase() : rest;
  }
  return spec;
}

const MODIFIER_KEYS = new Set(["Control", "Shift", "Alt", "Meta", "AltGraph", "CapsLock"]);

/** The binding a key press records: `null` for a bare modifier, which is not a binding. Shift is
 *  kept exactly as pressed, so a recorded binding matches that press and no other. */
export function specFromEvent(e: KeyboardEvent): KeySpec | null {
  if (MODIFIER_KEYS.has(e.key) || e.key === "Dead" || e.key === "Unidentified") {
    return null;
  }
  const spec: KeySpec = {};
  if (e.ctrlKey || e.metaKey) {
    spec.ctrl = true;
  }
  if (e.altKey) {
    spec.alt = true;
  }
  if (e.shiftKey) {
    spec.shift = true;
  }
  if (e.code === "Space") {
    spec.code = "Space";
  } else {
    spec.key = e.key.length === 1 ? e.key.toLowerCase() : e.key;
  }
  return spec;
}

const sameKey = (a: KeySpec, b: KeySpec) => {
  if (a.code !== undefined || b.code !== undefined) {
    return a.code === b.code;
  }
  return (a.key ?? "").toLowerCase() === (b.key ?? "").toLowerCase();
};

/** True when one key press would trigger both bindings. A binding that ignores Shift overlaps
 *  the same key with and without it. */
export function specsConflict(a: KeySpec, b: KeySpec): boolean {
  return (
    Boolean(a.ctrl) === Boolean(b.ctrl) &&
    Boolean(a.alt) === Boolean(b.alt) &&
    (a.anyShift === true || b.anyShift === true || Boolean(a.shift) === Boolean(b.shift)) &&
    sameKey(a, b)
  );
}

/** Bindings the window or the editing fields need for themselves. */
const RESERVED = [
  "Tab",
  "Shift+Tab",
  "Enter",
  "Alt+Tab",
  "Alt+F4",
  "Ctrl+W",
  "Ctrl+Q",
  "Ctrl+R",
  "Ctrl+Shift+R",
  "Ctrl+Shift+I",
  "Ctrl+Shift+J",
  "F5",
  "F11",
  "F12",
].map((text) => {
  return parseKeys(text) as KeySpec;
});

/** Why a binding cannot be used at all, or `null`. */
export function reservedReason(spec: KeySpec): TextKey | null {
  const clash = RESERVED.some((reserved) => {
    return specsConflict(reserved, spec);
  });
  return clash ? "keymapReserved" : null;
}

/** The bindings a command answers to now: the user's, if they changed it, else its defaults. */
export function effectiveKeys(
  id: string,
  defaults: KeySpec[] | undefined,
  overrides: Overrides,
): KeySpec[] {
  const stored = overrides[id];
  if (stored === undefined) {
    return defaults ?? [];
  }
  return stored
    .map((text) => {
      return parseKeys(text);
    })
    .filter((spec): spec is KeySpec => {
      return spec !== null;
    });
}

/** Other commands that already use a binding that overlaps `spec`. */
export function findConflicts(
  id: string,
  spec: KeySpec,
  commands: Bindable[],
  overrides: Overrides,
): Bindable[] {
  return commands.filter((command) => {
    return (
      command.id !== id &&
      effectiveKeys(command.id, command.keys, overrides).some((other) => {
        return specsConflict(other, spec);
      })
    );
  });
}

/** `Ctrl+Z`-style text for a binding, or an em dash for none. */
export function describeKeys(specs: KeySpec[]): string {
  return specs.length === 0
    ? "—"
    : specs
        .map((spec) => {
          return formatKeys(spec);
        })
        .join(", ");
}

export const keymapStore = createStore<Overrides>({});

/** What giving a command a binding would do. */
export type AddPlan =
  | { status: "added" }
  | { status: "conflict"; conflicts: Bindable[] }
  | { status: "reserved" }
  | { status: "duplicate" }
  | { status: "full" };

export function planAdd(
  id: string,
  spec: KeySpec,
  commands: Bindable[],
  overrides: Overrides,
): AddPlan {
  if (reservedReason(spec) !== null) {
    return { status: "reserved" };
  }
  const own = commands.find((command) => {
    return command.id === id;
  });
  const current = effectiveKeys(id, own?.keys, overrides);
  if (
    current.some((existing) => {
      return specsConflict(existing, spec);
    })
  ) {
    return { status: "duplicate" };
  }
  if (current.length >= MAX_BINDINGS_PER_COMMAND) {
    return { status: "full" };
  }
  const conflicts = findConflicts(id, spec, commands, overrides);
  return conflicts.length > 0 ? { status: "conflict", conflicts } : { status: "added" };
}

const serializeAll = (specs: KeySpec[]) => {
  return specs.map((spec) => {
    return serializeKeys(spec);
  });
};

/** Drops entries that merely restate a command's defaults, so "customized" stays accurate. */
export function normalizeOverrides(overrides: Overrides, commands: Bindable[]): Overrides {
  const next: Overrides = {};
  for (const [id, stored] of Object.entries(overrides)) {
    const command = commands.find((candidate) => {
      return candidate.id === id;
    });
    if (!command) {
      continue;
    }
    const defaults = serializeAll(command.keys ?? []);
    const same =
      stored.length === defaults.length &&
      stored.every((text, i) => {
        return text === defaults[i];
      });
    if (!same) {
      next[id] = stored;
    }
  }
  return next;
}

/** `overrides` with `spec` added to `id`; with `replaceConflicts`, the overlapping bindings of
 *  other commands are removed first (those commands lose that shortcut). */
export function applyAdd(
  id: string,
  spec: KeySpec,
  commands: Bindable[],
  overrides: Overrides,
  replaceConflicts: boolean,
): Overrides {
  const next: Overrides = { ...overrides };
  if (replaceConflicts) {
    for (const other of findConflicts(id, spec, commands, overrides)) {
      next[other.id] = serializeAll(
        effectiveKeys(other.id, other.keys, overrides).filter((existing) => {
          return !specsConflict(existing, spec);
        }),
      );
    }
  }
  const own = commands.find((command) => {
    return command.id === id;
  });
  next[id] = serializeAll([...effectiveKeys(id, own?.keys, overrides), spec]);
  return normalizeOverrides(next, commands);
}

/** `overrides` with the `index`th binding of `id` removed (the command may end up unbound). */
export function applyRemove(
  id: string,
  index: number,
  commands: Bindable[],
  overrides: Overrides,
): Overrides {
  const own = commands.find((command) => {
    return command.id === id;
  });
  const remaining = effectiveKeys(id, own?.keys, overrides).filter((_, i) => {
    return i !== index;
  });
  return normalizeOverrides({ ...overrides, [id]: serializeAll(remaining) }, commands);
}

/** `overrides` with `id` back at its defaults. */
export function applyReset(id: string, overrides: Overrides): Overrides {
  const next = { ...overrides };
  delete next[id];
  return next;
}

/** Applies `next` at once and saves it. The backend returns what it kept, which wins. */
export async function saveOverrides(next: Overrides): Promise<void> {
  keymapStore.set(next);
  const saved = await run(api.setKeybindings(next));
  if (saved) {
    keymapStore.set(saved);
  }
}

/** Loads the saved changes, dropping unknown commands and unreadable bindings. Returns how many
 *  entries were dropped (and saves the cleaned table when it was not zero). */
export async function loadKeymap(known: Set<string>): Promise<number> {
  let saved: Overrides | undefined;
  try {
    saved = await run(api.getKeybindings());
  } catch {
    return 0;
  }
  if (!saved) {
    return 0;
  }
  let dropped = 0;
  const clean: Overrides = {};
  for (const [id, stored] of Object.entries(saved)) {
    if (!known.has(id)) {
      dropped += 1;
      continue;
    }
    const valid = stored.filter((text) => {
      return parseKeys(text) !== null;
    });
    dropped += stored.length - valid.length;
    clean[id] = valid;
  }
  keymapStore.set(clean);
  if (dropped > 0) {
    await saveOverrides(clean);
  }
  return dropped;
}

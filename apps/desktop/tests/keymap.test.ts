// @vitest-environment jsdom
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { KeySpec } from "../src/shortcuts";

vi.mock("../src/ipc", () => {
  return {
    api: {
      getKeybindings: vi.fn(),
      setKeybindings: vi.fn(),
    },
  };
});

const keymap = await import("../src/keymap");
const { api } = await import("../src/ipc");

const rows = [
  { id: "undo", keys: [{ key: "z", ctrl: true }] },
  { id: "split", keys: [{ key: "s", anyShift: true }, { key: "c" }] },
  { id: "marker", keys: [{ key: "m" }] },
  { id: "free" },
];

beforeEach(() => {
  vi.clearAllMocks();
  keymap.keymapStore.set({});
});

describe("serialize and parse", () => {
  it.each<KeySpec>([
    { key: "z", ctrl: true, shift: true },
    { key: "m", alt: true },
    { code: "Space" },
    { key: "ArrowLeft", shift: true },
    { key: "s", anyShift: true },
    { key: "F2" },
    { key: "+", shift: true, ctrl: true },
    { key: "=", ctrl: true },
    { key: "\\" },
  ])("round-trips %j", (spec) => {
    expect(keymap.parseKeys(keymap.serializeKeys(spec))).toEqual(spec);
  });

  it("writes the stored form people would expect", () => {
    expect(keymap.serializeKeys({ key: "z", ctrl: true, shift: true })).toBe("Ctrl+Shift+Z");
    expect(keymap.serializeKeys({ key: "s", anyShift: true })).toBe("Shift?+S");
    expect(keymap.serializeKeys({ code: "Space" })).toBe("Space");
  });

  it("parses case-insensitively and rejects malformed text", () => {
    expect(keymap.parseKeys("ctrl+shift+z")).toEqual({ key: "z", ctrl: true, shift: true });
    expect(keymap.parseKeys("space")).toEqual({ code: "Space" });
    expect(keymap.parseKeys("")).toBeNull();
    expect(keymap.parseKeys("   ")).toBeNull();
    expect(keymap.parseKeys("Ctrl+")).toBeNull();
    expect(keymap.parseKeys("x".repeat(40))).toBeNull();
  });
});

describe("specFromEvent", () => {
  const press = (init: KeyboardEventInit) => {
    return new KeyboardEvent("keydown", init);
  };

  it("records the key with exactly the modifiers held", () => {
    expect(keymap.specFromEvent(press({ key: "Z", shiftKey: true, ctrlKey: true }))).toEqual({
      key: "z",
      ctrl: true,
      shift: true,
    });
    expect(keymap.specFromEvent(press({ key: " ", code: "Space" }))).toEqual({ code: "Space" });
    expect(keymap.specFromEvent(press({ key: "F2" }))).toEqual({ key: "F2" });
  });

  it("ignores a bare modifier or an unidentified key", () => {
    for (const key of ["Control", "Shift", "Alt", "Meta", "Dead", "Unidentified"]) {
      expect(keymap.specFromEvent(press({ key }))).toBeNull();
    }
  });
});

describe("conflicts and reserved keys", () => {
  it("overlaps when the same key is pressed with the same modifiers", () => {
    expect(keymap.specsConflict({ key: "s" }, { key: "S" })).toBe(true);
    expect(keymap.specsConflict({ key: "s" }, { key: "s", ctrl: true })).toBe(false);
    expect(keymap.specsConflict({ key: "s" }, { key: "s", alt: true })).toBe(false);
  });

  it("lets a Shift-agnostic binding overlap both variants", () => {
    expect(keymap.specsConflict({ key: "s", anyShift: true }, { key: "s", shift: true })).toBe(
      true,
    );
    expect(keymap.specsConflict({ key: "s", shift: true }, { key: "s" })).toBe(false);
  });

  it("compares keys by code when one has a code", () => {
    expect(keymap.specsConflict({ code: "Space" }, { code: "Space" })).toBe(true);
    expect(keymap.specsConflict({ code: "Space" }, { key: " " })).toBe(false);
  });

  it("refuses system and text-field keys", () => {
    for (const text of ["Ctrl+W", "Alt+F4", "Tab", "Enter", "F5", "Ctrl+Shift+I"]) {
      expect(keymap.reservedReason(keymap.parseKeys(text)!)).toBe("keymapReserved");
    }
    expect(keymap.reservedReason({ key: "q" })).toBeNull();
    expect(keymap.reservedReason({ key: "w", ctrl: true, shift: true })).toBeNull();
  });
});

describe("effective keys", () => {
  it("uses the defaults until the user changes a command", () => {
    expect(keymap.effectiveKeys("undo", rows[0].keys, {})).toEqual([{ key: "z", ctrl: true }]);
    expect(keymap.effectiveKeys("undo", rows[0].keys, { undo: ["Alt+U"] })).toEqual([
      { key: "u", alt: true },
    ]);
  });

  it("treats an empty list as unbound and skips unreadable entries", () => {
    expect(keymap.effectiveKeys("undo", rows[0].keys, { undo: [] })).toEqual([]);
    expect(keymap.effectiveKeys("undo", rows[0].keys, { undo: ["", "Alt+U"] })).toEqual([
      { key: "u", alt: true },
    ]);
    expect(keymap.effectiveKeys("free", undefined, {})).toEqual([]);
  });

  it("describes bindings or an em dash", () => {
    expect(keymap.describeKeys([{ key: "z", ctrl: true }, { key: "y" }])).toBe("Ctrl+Z, Y");
    expect(keymap.describeKeys([])).toBe("—");
  });
});

describe("adding a binding", () => {
  it("adds it to a command's current bindings when nothing overlaps", () => {
    expect(keymap.planAdd("marker", { key: "q" }, rows, {})).toEqual({ status: "added" });
    expect(keymap.applyAdd("marker", { key: "q" }, rows, {}, false)).toEqual({
      marker: ["M", "Q"],
    });
  });

  it("reports the commands a binding would take over", () => {
    const plan = keymap.planAdd("free", { key: "z", ctrl: true }, rows, {});
    expect(plan.status).toBe("conflict");
    expect(
      plan.status === "conflict" &&
        plan.conflicts.map((c) => {
          return c.id;
        }),
    ).toEqual(["undo"]);
  });

  it("sees a Shift-agnostic default as overlapping the shifted key", () => {
    const plan = keymap.planAdd("free", { key: "s", shift: true }, rows, {});
    expect(
      plan.status === "conflict" &&
        plan.conflicts.map((c) => {
          return c.id;
        }),
    ).toEqual(["split"]);
  });

  it("takes the binding from the other command when told to replace", () => {
    const next = keymap.applyAdd("free", { key: "z", ctrl: true }, rows, {}, true);
    expect(next).toEqual({ free: ["Ctrl+Z"], undo: [] });
  });

  it("refuses reserved, duplicate and surplus bindings", () => {
    expect(keymap.planAdd("free", { key: "w", ctrl: true }, rows, {}).status).toBe("reserved");
    expect(keymap.planAdd("marker", { key: "m" }, rows, {}).status).toBe("duplicate");
    const full = { free: ["A", "B", "C", "D"] };
    expect(keymap.planAdd("free", { key: "e" }, rows, full).status).toBe("full");
  });

  it("ignores its own bindings when looking for conflicts", () => {
    expect(keymap.findConflicts("undo", { key: "z", ctrl: true }, rows, {})).toEqual([]);
  });
});

describe("removing, resetting and normalizing", () => {
  it("removes one binding and leaves the rest", () => {
    expect(keymap.applyRemove("split", 0, rows, {})).toEqual({ split: ["C"] });
  });

  it("can leave a command with no binding at all", () => {
    expect(keymap.applyRemove("marker", 0, rows, {})).toEqual({ marker: [] });
  });

  it("drops an override that restates the defaults", () => {
    expect(keymap.normalizeOverrides({ marker: ["M"], undo: ["Alt+U"] }, rows)).toEqual({
      undo: ["Alt+U"],
    });
    expect(keymap.applyRemove("marker", 1, rows, { marker: ["M", "Q"] })).toEqual({});
  });

  it("drops overrides for commands that do not exist", () => {
    expect(keymap.normalizeOverrides({ gone: ["A"] }, rows)).toEqual({});
  });

  it("restores one command without touching the others", () => {
    expect(keymap.applyReset("undo", { undo: [], marker: ["Q"] })).toEqual({ marker: ["Q"] });
  });
});

describe("saving and loading", () => {
  it("applies a change at once and keeps what the backend returns", async () => {
    vi.mocked(api.setKeybindings).mockResolvedValue({ marker: ["Q"] });
    const save = keymap.saveOverrides({ marker: ["Q", "bad key!!"] });
    expect(keymap.keymapStore.get()).toEqual({ marker: ["Q", "bad key!!"] });
    await save;
    expect(api.setKeybindings).toHaveBeenCalledWith({ marker: ["Q", "bad key!!"] });
    expect(keymap.keymapStore.get()).toEqual({ marker: ["Q"] });
  });

  it("puts the previous table back when the save fails", async () => {
    keymap.keymapStore.set({ undo: ["U"] });
    vi.mocked(api.setKeybindings).mockRejectedValue(new Error("disk full"));
    const save = keymap.saveOverrides({ undo: ["U"], marker: ["Q"] });
    expect(keymap.keymapStore.get()).toEqual({ undo: ["U"], marker: ["Q"] });
    await save;
    expect(keymap.keymapStore.get()).toEqual({ undo: ["U"] });
  });

  it("lets only the latest save decide, whatever order the answers arrive in", async () => {
    const answers: ((table: Record<string, string[]>) => void)[] = [];
    vi.mocked(api.setKeybindings).mockImplementation(() => {
      return new Promise((resolve) => {
        answers.push(resolve);
      });
    });
    const first = keymap.saveOverrides({ marker: ["A"] });
    const second = keymap.saveOverrides({ marker: ["B"] });
    answers[1]({ marker: ["B"] });
    await second;
    answers[0]({ marker: ["A"] });
    await first;
    expect(keymap.keymapStore.get()).toEqual({ marker: ["B"] });
  });

  it("loads saved changes and drops unknown commands and unreadable bindings", async () => {
    vi.mocked(api.getKeybindings).mockResolvedValue({ marker: ["Q", ""], gone: ["A"] });
    vi.mocked(api.setKeybindings).mockImplementation((table) => {
      return Promise.resolve(table);
    });
    const dropped = await keymap.loadKeymap(new Set(["marker", "undo"]));
    expect(dropped).toBe(2);
    expect(keymap.keymapStore.get()).toEqual({ marker: ["Q"] });
    expect(api.setKeybindings).toHaveBeenCalledWith({ marker: ["Q"] });
  });

  it("does not rewrite a clean table", async () => {
    vi.mocked(api.getKeybindings).mockResolvedValue({ marker: ["Q"] });
    expect(await keymap.loadKeymap(new Set(["marker"]))).toBe(0);
    expect(api.setKeybindings).not.toHaveBeenCalled();
  });

  it("copes with a backend that cannot answer", async () => {
    vi.mocked(api.getKeybindings).mockRejectedValue(new Error("down"));
    expect(await keymap.loadKeymap(new Set(["marker"]))).toBe(0);
    expect(keymap.keymapStore.get()).toEqual({});
  });
});

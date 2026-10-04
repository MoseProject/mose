// @vitest-environment jsdom
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("./ipc", () => {
  const names = [
    "setPlaying",
    "seek",
    "step",
    "undo",
    "redo",
    "splitAtPlayhead",
    "deleteClips",
    "copyClip",
    "cutClip",
    "pasteClip",
    "addMarker",
    "rippleDeleteRange",
    "liftRange",
    "saveProject",
    "importMediaDialog",
    "copyClipFormatting",
    "pasteClipFormatting",
  ];
  return {
    api: Object.fromEntries(
      names.map((name) => {
        return [name, vi.fn().mockResolvedValue(name === "pasteClip" ? [5] : undefined)];
      }),
    ),
  };
});

const commands = await import("./commands");
const { api } = await import("./ipc");
const { keymapStore } = await import("./keymap");
const { reservedReason, specsConflict } = await import("./keymap");
const { previewStore, sessionStore, uiStore } = await import("./store");
const { makeSession } = await import("./testSupport");
const { t } = await import("./i18n");

const press = (init: KeyboardEventInit) => {
  return new KeyboardEvent("keydown", { cancelable: true, ...init });
};
const run = (id: string) => {
  return commands.findCommand(id)!.run();
};

beforeEach(() => {
  vi.clearAllMocks();
  keymapStore.set({});
  sessionStore.set(null);
  uiStore.set({ ...uiStore.get(), selectedClipIds: [], markIn: null, markOut: null });
});

describe("the command table", () => {
  it("has unique ids and a translated label for every command", () => {
    const ids = commands.COMMANDS.map((command) => {
      return command.id;
    });
    expect(new Set(ids).size).toBe(ids.length);
    for (const command of commands.COMMANDS) {
      expect(t(command.label), command.id).not.toBe("");
    }
  });

  it("never gives two commands overlapping default bindings", () => {
    const seen: { id: string; spec: Parameters<typeof specsConflict>[0] }[] = [];
    for (const command of commands.COMMANDS) {
      for (const spec of command.keys ?? []) {
        const clash = seen.find((other) => {
          return other.id !== command.id && specsConflict(other.spec, spec);
        });
        expect(clash && [clash.id, command.id], JSON.stringify(spec)).toBeUndefined();
        seen.push({ id: command.id, spec });
      }
    }
  });

  it("never binds a default that the shortcut editor itself would refuse", () => {
    for (const command of commands.COMMANDS) {
      for (const spec of command.keys ?? []) {
        expect(reservedReason(spec), command.id).toBeNull();
      }
    }
  });
});

describe("key lookup", () => {
  it("finds the command for a key press", () => {
    expect(commands.commandForEvent(press({ key: "z", ctrlKey: true }))?.id).toBe("undo");
    expect(commands.commandForEvent(press({ key: "z", ctrlKey: true, shiftKey: true }))?.id).toBe(
      "redo",
    );
    expect(commands.commandForEvent(press({ key: " ", code: "Space" }))?.id).toBe("playPause");
    expect(commands.commandForEvent(press({ key: "S", shiftKey: true }))?.id).toBe("split");
    expect(commands.commandForEvent(press({ key: "q" }))).toBeUndefined();
  });

  it("tells plain, shifted and alt variants apart", () => {
    expect(commands.commandForEvent(press({ key: "m" }))?.id).toBe("addMarker");
    expect(commands.commandForEvent(press({ key: "M", shiftKey: true }))?.id).toBe("prevMarker");
    expect(commands.commandForEvent(press({ key: "m", altKey: true }))?.id).toBe("nextMarker");
    expect(commands.commandForEvent(press({ key: "Delete" }))?.id).toBe("delete");
    expect(commands.commandForEvent(press({ key: "Delete", shiftKey: true }))?.id).toBe(
      "rippleDelete",
    );
    expect(
      commands.commandForEvent(press({ key: "Delete", ctrlKey: true, shiftKey: true }))?.id,
    ).toBe("deleteRange");
  });

  it("shows the first binding in labels and tooltips", () => {
    expect(commands.shortcutLabel("undo")).toBe("Ctrl+Z");
    expect(commands.shortcutLabel("redo")).toBe("Ctrl+Shift+Z");
    expect(commands.shortcutLabel("scrollToPlayhead")).toBeUndefined();
    expect(commands.shortcutLabel("nope")).toBeUndefined();
    expect(commands.withShortcut("Undo", "undo")).toBe("Undo (Ctrl+Z)");
    expect(commands.withShortcut("Plain", "scrollToPlayhead")).toBe("Plain");
  });

  it("follows the user's changes over the defaults", () => {
    keymapStore.set({ undo: ["Alt+U"], addMarker: [] });
    expect(commands.commandForEvent(press({ key: "z", ctrlKey: true }))).toBeUndefined();
    expect(commands.commandForEvent(press({ key: "u", altKey: true }))?.id).toBe("undo");
    expect(commands.shortcutLabel("undo")).toBe("Alt+U");
    expect(commands.commandForEvent(press({ key: "m" }))).toBeUndefined();
    expect(commands.shortcutLabel("addMarker")).toBeUndefined();
  });

  it("lists every command as a bindable row", () => {
    const rows = commands.bindables();
    expect(rows.length).toBe(commands.COMMANDS.length);
    expect(
      rows.find((row) => {
        return row.id === "undo";
      })?.keys,
    ).toEqual([{ key: "z", ctrl: true }]);
  });
});

describe("commands that need a selection or a session", () => {
  it("is enabled only with something selected", () => {
    const del = commands.findCommand("delete")!;
    expect(del.enabled?.()).toBe(false);
    uiStore.set({ ...uiStore.get(), selectedClipIds: [3] });
    expect(del.enabled?.()).toBe(true);
  });

  it("routes delete, ripple delete and clipboard to the selection", async () => {
    uiStore.set({ ...uiStore.get(), selectedClipIds: [3, 4] });
    await run("delete");
    await run("rippleDelete");
    await run("copy");
    await run("cut");
    expect(api.deleteClips).toHaveBeenCalledWith([3, 4], false);
    expect(api.deleteClips).toHaveBeenCalledWith([3, 4], true);
    expect(api.copyClip).toHaveBeenCalledWith(3);
    expect(api.cutClip).toHaveBeenCalledWith(3);
  });

  it("duplicates by copying then pasting and selects the result", async () => {
    uiStore.set({ ...uiStore.get(), selectedClipIds: [3] });
    await run("duplicate");
    expect(api.copyClip).toHaveBeenCalledWith(3);
    expect(api.pasteClip).toHaveBeenCalled();
    expect(uiStore.get().selectedClipIds).toEqual([5]);
  });

  it("selects every clip, then clears the selection", async () => {
    sessionStore.set(
      makeSession([
        {
          id: 1,
          name: "S",
          tracks: [
            { id: 1, clips: [{ id: 7 }, { id: 8 }], text_clips: [{ id: 9 }], shape_clips: [] },
          ],
        },
      ]),
    );
    await run("selectAll");
    expect(uiStore.get().selectedClipIds.sort()).toEqual([7, 8, 9]);
    await run("deselect");
    expect(uiStore.get().selectedClipIds).toEqual([]);
  });
});

describe("in and out marks", () => {
  const playhead = (secs: number) => {
    previewStore.set({ ...previewStore.get(), playheadSecs: secs });
  };

  it("marks the playhead position", async () => {
    playhead(4.5);
    await run("markIn");
    playhead(9);
    await run("markOut");
    expect(uiStore.get().markIn).toBe(4.5);
    expect(uiStore.get().markOut).toBe(9);
    await run("clearMarks");
    expect(uiStore.get().markIn).toBeNull();
    expect(uiStore.get().markOut).toBeNull();
  });

  it("lifts the range, leaving the gap, only when both marks are set in order", async () => {
    const lift = commands.findCommand("liftRange")!;
    expect(lift.enabled?.()).toBe(false);
    await lift.run();
    expect(api.liftRange).not.toHaveBeenCalled();

    uiStore.set({ ...uiStore.get(), markIn: 2, markOut: 6 });
    expect(lift.enabled?.()).toBe(true);
    await lift.run();
    expect(api.liftRange).toHaveBeenCalledWith(2, 6);
    expect(uiStore.get().markIn).toBeNull();
  });

  it("jumps to the marks once they exist", async () => {
    const toIn = commands.findCommand("goToMarkIn")!;
    const toOut = commands.findCommand("goToMarkOut")!;
    expect(toIn.enabled?.()).toBe(false);
    expect(toOut.enabled?.()).toBe(false);
    uiStore.set({ ...uiStore.get(), markIn: 3, markOut: 8 });
    await toIn.run();
    await toOut.run();
    expect(api.seek).toHaveBeenNthCalledWith(1, 3);
    expect(api.seek).toHaveBeenNthCalledWith(2, 8);
  });

  it("deletes the range only when both marks are set in order", async () => {
    const deleteRange = commands.findCommand("deleteRange")!;
    expect(deleteRange.enabled?.()).toBe(false);
    uiStore.set({ ...uiStore.get(), markIn: 6, markOut: 2 });
    expect(deleteRange.enabled?.()).toBe(false);
    await deleteRange.run();
    expect(api.rippleDeleteRange).not.toHaveBeenCalled();

    uiStore.set({ ...uiStore.get(), markIn: 2, markOut: 6 });
    expect(deleteRange.enabled?.()).toBe(true);
    await deleteRange.run();
    expect(api.rippleDeleteRange).toHaveBeenCalledWith(2, 6);
    expect(uiStore.get().markIn).toBeNull();
    expect(uiStore.get().markOut).toBeNull();
  });
});

describe("playback", () => {
  const playhead = (secs: number) => {
    previewStore.set({ ...previewStore.get(), playheadSecs: secs, playing: false });
  };

  it("jumps a second or ten seconds, never before zero", async () => {
    playhead(5);
    await run("back1s");
    await run("forward10s");
    await run("back10s");
    expect(api.seek).toHaveBeenNthCalledWith(1, 4);
    expect(api.seek).toHaveBeenNthCalledWith(2, 15);
    expect(api.seek).toHaveBeenNthCalledWith(3, 0);
  });

  it("shuttles with J and L and stops on K", async () => {
    const { shuttleStore } = await import("./shuttle");
    await run("shuttleBack");
    expect(shuttleStore.get()).toEqual({ direction: -1, level: 1 });
    await run("play");
    expect(shuttleStore.get()).toEqual({ direction: 1, level: 1 });
    await run("pause");
    expect(shuttleStore.get().direction).toBe(0);
  });

  it("plays, pauses and toggles", async () => {
    await run("play");
    await run("pause");
    await run("playPause");
    expect(api.setPlaying).toHaveBeenNthCalledWith(1, true);
    expect(api.setPlaying).toHaveBeenNthCalledWith(2, false);
    expect(api.setPlaying).toHaveBeenNthCalledWith(3, true);
  });

  it("goes to the end of the timeline and between cut points", async () => {
    sessionStore.set(
      makeSession([
        {
          id: 1,
          name: "S",
          tracks: [
            {
              id: 1,
              clips: [
                { id: 1, start_secs: 2, source_in_secs: 0, source_out_secs: 3, speed_factor: 1 },
              ],
              text_clips: [],
              shape_clips: [],
            },
          ],
        },
      ]),
    );
    playhead(2);
    await run("goToEnd");
    expect(api.seek).toHaveBeenLastCalledWith(5);
    await run("nextEdit");
    expect(api.seek).toHaveBeenLastCalledWith(5);
    await run("prevEdit");
    expect(api.seek).toHaveBeenLastCalledWith(0);
  });
});

describe("tools and toggles", () => {
  it("switches the active tool", async () => {
    await run("toolRoll");
    expect(uiStore.get().tool).toBe("roll");
    await run("toolSelect");
    expect(uiStore.get().tool).toBe("select");
  });

  it("toggles snapping and the monitor guides", async () => {
    const snap = uiStore.get().snapEnabled;
    await run("toggleSnap");
    expect(uiStore.get().snapEnabled).toBe(!snap);
    await run("toggleSnap");
    const guides = uiStore.get().monitorGuides;
    await run("toggleGuides");
    expect(uiStore.get().monitorGuides).toBe(!guides);
  });

  it("opens the sheet, the palette, the settings and the export dialog", async () => {
    await run("shortcuts");
    await run("palette");
    await run("settings");
    await run("export");
    const ui = uiStore.get();
    expect([ui.shortcutsOpen, ui.paletteOpen, ui.settingsOpen, ui.exportDialogOpen]).toEqual([
      true,
      true,
      true,
      true,
    ]);
    uiStore.set({
      ...ui,
      shortcutsOpen: false,
      paletteOpen: false,
      settingsOpen: false,
      exportDialogOpen: false,
    });
  });
});

describe("repeatable commands", () => {
  it("only lets stepping, seeking, zooming, undo and navigation run while a key is held", () => {
    const repeatable = commands.COMMANDS.filter((command) => {
      return command.repeatable;
    }).map((command) => {
      return command.id;
    });
    expect(repeatable.sort()).toEqual(
      [
        "back1s",
        "back10s",
        "forward10s",
        "stepBack",
        "stepForward",
        "stepBack10",
        "stepForward10",
        "undo",
        "redo",
        "zoomIn",
        "zoomOut",
        "prevEdit",
        "nextEdit",
        "prevMarker",
        "nextMarker",
      ].sort(),
    );
  });

  it("never repeats playback toggles, edits or the shuttle", () => {
    for (const id of ["playPause", "play", "shuttleBack", "split", "delete", "addMarker"]) {
      expect(commands.findCommand(id)?.repeatable).toBeFalsy();
    }
  });
});

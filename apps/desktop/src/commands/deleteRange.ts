import { api } from "../ipc";
import { patchUi, run, uiStore } from "../store";
import type { Command } from "./types";

export const deleteRangeCommand: Command = {
  id: "deleteRange",
  label: "cmdDeleteRange",
  group: "timeline",
  keys: [{ key: "Delete", ctrl: true, shift: true }],
  enabled: () => {
    const { markIn, markOut } = uiStore.get();
    return markIn !== null && markOut !== null && markOut > markIn;
  },
  run: async () => {
    const { markIn, markOut } = uiStore.get();
    if (markIn === null || markOut === null || markOut <= markIn) {
      return;
    }
    await run(api.rippleDeleteRange(markIn, markOut));
    patchUi({ markIn: null, markOut: null });
  },
};

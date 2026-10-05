import { api } from "../ipc";
import { patchUi, run, uiStore } from "../store";
import type { Command } from "./types";

export const liftRangeCommand: Command = {
  id: "liftRange",
  label: "cmdLiftRange",
  group: "timeline",
  keys: [{ key: ";" }],
  enabled: () => {
    const { markIn, markOut } = uiStore.get();
    return markIn !== null && markOut !== null && markOut > markIn;
  },
  run: async () => {
    const { markIn, markOut } = uiStore.get();
    if (markIn === null || markOut === null || markOut <= markIn) {
      return;
    }
    await run(api.liftRange(markIn, markOut));
    patchUi({ markIn: null, markOut: null });
  },
};

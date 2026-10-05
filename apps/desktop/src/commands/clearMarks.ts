import { patchUi } from "../store";
import type { Command } from "./types";

export const clearMarksCommand: Command = {
  id: "clearMarks",
  label: "cmdClearMarks",
  group: "timeline",
  keys: [{ key: "x", alt: true }],
  run: () => {
    patchUi({ markIn: null, markOut: null });
  },
};

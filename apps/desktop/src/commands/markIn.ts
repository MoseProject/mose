import { patchUi, previewStore } from "../store";
import type { Command } from "./types";

export const markInCommand: Command = {
  id: "markIn",
  label: "cmdMarkIn",
  group: "timeline",
  keys: [{ key: "i" }],
  run: () => {
    patchUi({ markIn: previewStore.get().playheadSecs });
  },
};

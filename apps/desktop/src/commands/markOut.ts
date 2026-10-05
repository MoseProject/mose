import { patchUi, previewStore } from "../store";
import type { Command } from "./types";

export const markOutCommand: Command = {
  id: "markOut",
  label: "cmdMarkOut",
  group: "timeline",
  keys: [{ key: "o" }],
  run: () => {
    patchUi({ markOut: previewStore.get().playheadSecs });
  },
};

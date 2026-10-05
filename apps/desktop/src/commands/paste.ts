import { api } from "../ipc";
import { patchUi, run } from "../store";
import type { Command } from "./types";

export const pasteCommand: Command = {
  id: "paste",
  label: "menuPaste",
  group: "edit",
  keys: [{ key: "v", ctrl: true }],
  run: () => {
    return run(api.pasteClip()).then((ids) => {
      if (ids && ids.length > 0) {
        patchUi({ selectedClipIds: ids });
      }
    });
  },
};

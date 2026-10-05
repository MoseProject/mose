import { api } from "../ipc";
import { patchUi, run } from "../store";
import type { Command } from "./types";
import { selected, hasSelection } from "./helpers";

export const duplicateCommand: Command = {
  id: "duplicate",
  label: "cmdDuplicate",
  group: "edit",
  keys: [{ key: "d", ctrl: true }],
  enabled: hasSelection,
  run: async () => {
    await run(api.copyClip(selected()[0]));
    const ids = await run(api.pasteClip());
    if (ids && ids.length > 0) {
      patchUi({ selectedClipIds: ids });
    }
  },
};

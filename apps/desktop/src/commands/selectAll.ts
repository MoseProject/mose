import { allClipIds } from "../editing";
import { patchUi, sessionStore } from "../store";
import type { Command } from "./types";
import { hasSession } from "./helpers";

export const selectAllCommand: Command = {
  id: "selectAll",
  label: "cmdSelectAll",
  group: "edit",
  keys: [{ key: "a", ctrl: true }],
  enabled: hasSession,
  run: () => {
    const session = sessionStore.get();
    if (session) {
      patchUi({ selectedClipIds: allClipIds(session) });
    }
  },
};

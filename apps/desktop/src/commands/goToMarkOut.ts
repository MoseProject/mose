import { api } from "../ipc";
import { run, uiStore } from "../store";
import type { Command } from "./types";

export const goToMarkOutCommand: Command = {
  id: "goToMarkOut",
  label: "cmdGoToMarkOut",
  group: "timeline",
  keys: [{ key: "o", shift: true }],
  enabled: () => {
    return uiStore.get().markOut !== null;
  },
  run: () => {
    const { markOut } = uiStore.get();
    return markOut === null ? undefined : run(api.seek(markOut));
  },
};

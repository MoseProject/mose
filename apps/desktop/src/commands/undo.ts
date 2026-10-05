import { api } from "../ipc";
import { run } from "../store";
import type { Command } from "./types";

export const undoCommand: Command = {
  id: "undo",
  repeatable: true,
  label: "undo",
  group: "edit",
  keys: [{ key: "z", ctrl: true }],
  run: () => {
    return run(api.undo());
  },
};

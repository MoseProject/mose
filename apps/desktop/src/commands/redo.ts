import { api } from "../ipc";
import { run } from "../store";
import type { Command } from "./types";

export const redoCommand: Command = {
  id: "redo",
  repeatable: true,
  label: "redo",
  group: "edit",
  keys: [
    { key: "z", ctrl: true, shift: true },
    { key: "y", ctrl: true },
  ],
  run: () => {
    return run(api.redo());
  },
};

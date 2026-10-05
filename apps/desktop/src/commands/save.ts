import { api } from "../ipc";
import { run } from "../store";
import type { Command } from "./types";

export const saveCommand: Command = {
  id: "save",
  label: "save",
  group: "file",
  keys: [{ key: "s", ctrl: true }],
  run: () => {
    return run(api.saveProject(false));
  },
};

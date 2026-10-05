import { api } from "../ipc";
import { run } from "../store";
import type { Command } from "./types";

export const saveAsCommand: Command = {
  id: "saveAs",
  label: "saveAs",
  group: "file",
  keys: [{ key: "s", ctrl: true, shift: true }],
  run: () => {
    return run(api.saveProject(true));
  },
};

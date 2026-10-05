import { api } from "../ipc";
import { run } from "../store";
import type { Command } from "./types";

export const importCommand: Command = {
  id: "import",
  label: "importFiles",
  group: "file",
  keys: [{ key: "i", ctrl: true }],
  run: () => {
    return run(api.importMediaDialog());
  },
};

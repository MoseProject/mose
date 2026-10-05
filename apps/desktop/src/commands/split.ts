import { api } from "../ipc";
import { run } from "../store";
import type { Command } from "./types";
import { selected } from "./helpers";

export const splitCommand: Command = {
  id: "split",
  label: "cmdSplit",
  group: "edit",
  keys: [{ key: "s", anyShift: true }, { key: "c" }],
  run: () => {
    return run(api.splitAtPlayhead(selected()));
  },
};

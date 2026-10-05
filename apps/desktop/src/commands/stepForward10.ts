import { api } from "../ipc";
import { run } from "../store";
import type { Command } from "./types";

export const stepForward10Command: Command = {
  id: "stepForward10",
  repeatable: true,
  label: "cmdStepForward10",
  group: "playback",
  keys: [{ key: "ArrowRight", shift: true }],
  palette: false,
  run: () => {
    return run(api.step(10));
  },
};

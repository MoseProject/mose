import { api } from "../ipc";
import { run } from "../store";
import type { Command } from "./types";

export const stepBack10Command: Command = {
  id: "stepBack10",
  repeatable: true,
  label: "cmdStepBack10",
  group: "playback",
  keys: [{ key: "ArrowLeft", shift: true }],
  palette: false,
  run: () => {
    return run(api.step(-10));
  },
};

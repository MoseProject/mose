import { api } from "../ipc";
import { run } from "../store";
import type { Command } from "./types";

export const stepForwardCommand: Command = {
  id: "stepForward",
  repeatable: true,
  label: "stepForward",
  group: "playback",
  keys: [{ key: "ArrowRight" }],
  run: () => {
    return run(api.step(1));
  },
};

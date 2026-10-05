import { api } from "../ipc";
import { run } from "../store";
import type { Command } from "./types";

export const stepBackCommand: Command = {
  id: "stepBack",
  repeatable: true,
  label: "stepBack",
  group: "playback",
  keys: [{ key: "ArrowLeft" }],
  run: () => {
    return run(api.step(-1));
  },
};

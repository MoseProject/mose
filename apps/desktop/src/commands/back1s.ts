import type { Command } from "./types";
import { seekBy } from "./helpers";

export const back1sCommand: Command = {
  id: "back1s",
  repeatable: true,
  label: "cmdBack1s",
  group: "playback",
  run: () => {
    return seekBy(-1);
  },
};

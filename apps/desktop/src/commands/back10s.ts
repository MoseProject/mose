import type { Command } from "./types";
import { seekBy } from "./helpers";

export const back10sCommand: Command = {
  id: "back10s",
  repeatable: true,
  label: "cmdBack10s",
  group: "playback",
  keys: [{ key: "j", shift: true }],
  palette: false,
  run: () => {
    return seekBy(-10);
  },
};

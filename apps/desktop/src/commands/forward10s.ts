import type { Command } from "./types";
import { seekBy } from "./helpers";

export const forward10sCommand: Command = {
  id: "forward10s",
  repeatable: true,
  label: "cmdForward10s",
  group: "playback",
  keys: [{ key: "l", shift: true }],
  palette: false,
  run: () => {
    return seekBy(10);
  },
};

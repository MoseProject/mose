import { shuttle } from "../shuttle";
import type { Command } from "./types";

export const playCommand: Command = {
  id: "play",
  label: "cmdPlay",
  group: "playback",
  keys: [{ key: "l" }],
  run: () => {
    return shuttle(1);
  },
};

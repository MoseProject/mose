import { shuttle } from "../shuttle";
import type { Command } from "./types";

export const shuttleBackCommand: Command = {
  id: "shuttleBack",
  label: "cmdShuttleBack",
  group: "playback",
  keys: [{ key: "j" }],
  run: () => {
    return shuttle(-1);
  },
};

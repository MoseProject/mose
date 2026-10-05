import { playRange } from "../shuttle";
import { uiStore } from "../store";
import type { Command } from "./types";

export const playRangeCommand: Command = {
  id: "playRange",
  label: "cmdPlayRange",
  group: "playback",
  keys: [{ key: "p", shift: true }],
  enabled: () => {
    const { markIn, markOut } = uiStore.get();
    return markIn !== null && markOut !== null && markOut > markIn;
  },
  run: () => {
    return playRange();
  },
};

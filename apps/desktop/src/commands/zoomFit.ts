import { timelineAction } from "../editing";
import type { Command } from "./types";

export const zoomFitCommand: Command = {
  id: "zoomFit",
  label: "zoomFit",
  group: "timeline",
  keys: [{ key: "z", shift: true }, { key: "\\" }],
  run: () => {
    timelineAction("zoomFit");
  },
};

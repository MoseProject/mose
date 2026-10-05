import { timelineAction } from "../editing";
import type { Command } from "./types";

export const zoomOutCommand: Command = {
  id: "zoomOut",
  repeatable: true,
  label: "zoomOut",
  group: "timeline",
  keys: [{ key: "-" }, { key: "-", ctrl: true }],
  run: () => {
    timelineAction("zoomOut");
  },
};

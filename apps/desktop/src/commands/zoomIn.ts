import { timelineAction } from "../editing";
import type { Command } from "./types";

export const zoomInCommand: Command = {
  id: "zoomIn",
  repeatable: true,
  label: "zoomIn",
  group: "timeline",
  keys: [{ key: "=" }, { key: "+", shift: true }, { key: "=", ctrl: true }],
  run: () => {
    timelineAction("zoomIn");
  },
};

import { timelineAction } from "../editing";
import type { Command } from "./types";

export const scrollToPlayheadCommand: Command = {
  id: "scrollToPlayhead",
  label: "cmdScrollToPlayhead",
  group: "timeline",
  run: () => {
    timelineAction("scrollToPlayhead");
  },
};

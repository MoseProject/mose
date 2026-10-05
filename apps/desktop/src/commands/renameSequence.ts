import { timelineAction } from "../editing";
import type { Command } from "./types";
import { hasSession } from "./helpers";

export const renameSequenceCommand: Command = {
  id: "renameSequence",
  label: "cmdRenameSequence",
  group: "timeline",
  keys: [{ key: "F2" }],
  enabled: hasSession,
  run: () => {
    timelineAction("renameSequence");
  },
};

import type { Command } from "./types";
import { setTool } from "./helpers";

export const toolRollCommand: Command = {
  id: "toolRoll",
  label: "cmdToolRoll",
  group: "tools",
  keys: [{ key: "r" }],
  run: setTool("roll"),
};

import type { Command } from "./types";
import { setTool } from "./helpers";

export const toolSlipCommand: Command = {
  id: "toolSlip",
  label: "cmdToolSlip",
  group: "tools",
  keys: [{ key: "y" }],
  run: setTool("slip"),
};

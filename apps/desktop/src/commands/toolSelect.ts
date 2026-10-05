import type { Command } from "./types";
import { setTool } from "./helpers";

export const toolSelectCommand: Command = {
  id: "toolSelect",
  label: "cmdToolSelect",
  group: "tools",
  keys: [{ key: "v" }],
  run: setTool("select"),
};

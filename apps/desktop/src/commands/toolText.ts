import type { Command } from "./types";
import { setTool } from "./helpers";

export const toolTextCommand: Command = {
  id: "toolText",
  label: "cmdToolText",
  group: "tools",
  keys: [{ key: "t" }],
  run: setTool("text"),
};

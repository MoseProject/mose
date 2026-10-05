import type { Command } from "./types";
import { setTool } from "./helpers";

export const toolSlideCommand: Command = {
  id: "toolSlide",
  label: "cmdToolSlide",
  group: "tools",
  keys: [{ key: "u" }],
  run: setTool("slide"),
};

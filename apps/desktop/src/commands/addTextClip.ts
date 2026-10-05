import { addTextOverlay } from "../editing";
import type { Command } from "./types";
import { hasSession } from "./helpers";

export const addTextClipCommand: Command = {
  id: "addTextClip",
  label: "addTextClip",
  group: "edit",
  enabled: hasSession,
  run: () => {
    return addTextOverlay();
  },
};

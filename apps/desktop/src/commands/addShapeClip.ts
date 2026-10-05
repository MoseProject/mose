import { addShapeOverlay } from "../editing";
import type { Command } from "./types";
import { hasSession } from "./helpers";

export const addShapeClipCommand: Command = {
  id: "addShapeClip",
  label: "addShapeClip",
  group: "edit",
  enabled: hasSession,
  run: () => {
    return addShapeOverlay();
  },
};

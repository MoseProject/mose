import { seekToMarker } from "../editing";
import type { Command } from "./types";
import { hasSession, activeMarkers } from "./helpers";

export const prevMarkerCommand: Command = {
  id: "prevMarker",
  repeatable: true,
  label: "previousMarker",
  group: "timeline",
  keys: [{ key: "m", shift: true }],
  enabled: hasSession,
  run: () => {
    seekToMarker(activeMarkers(), -1);
  },
};

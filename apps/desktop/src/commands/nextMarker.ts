import { seekToMarker } from "../editing";
import type { Command } from "./types";
import { hasSession, activeMarkers } from "./helpers";

export const nextMarkerCommand: Command = {
  id: "nextMarker",
  repeatable: true,
  label: "nextMarker",
  group: "timeline",
  keys: [{ key: "m", alt: true }],
  enabled: hasSession,
  run: () => {
    seekToMarker(activeMarkers(), 1);
  },
};

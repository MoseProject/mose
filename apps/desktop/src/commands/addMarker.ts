import { api } from "../ipc";
import { run } from "../store";
import type { Command } from "./types";

export const addMarkerCommand: Command = {
  id: "addMarker",
  label: "addMarker",
  group: "timeline",
  keys: [{ key: "m" }],
  run: () => {
    return run(api.addMarker());
  },
};

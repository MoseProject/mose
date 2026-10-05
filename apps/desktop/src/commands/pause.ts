import { api } from "../ipc";
import { stopShuttle } from "../shuttle";
import { run } from "../store";
import type { Command } from "./types";

export const pauseCommand: Command = {
  id: "pause",
  label: "cmdPause",
  group: "playback",
  keys: [{ key: "k" }],
  run: () => {
    stopShuttle();
    return run(api.setPlaying(false));
  },
};

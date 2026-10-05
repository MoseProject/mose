import { api } from "../ipc";
import { stopShuttle } from "../shuttle";
import { previewStore, run } from "../store";
import type { Command } from "./types";

export const playPauseCommand: Command = {
  id: "playPause",
  label: "cmdPlayPause",
  group: "playback",
  keys: [{ code: "Space" }],
  run: () => {
    stopShuttle();
    return run(api.setPlaying(!previewStore.get().playing));
  },
};

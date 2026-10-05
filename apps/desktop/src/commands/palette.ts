import { patchUi } from "../store";
import type { Command } from "./types";

export const paletteCommand: Command = {
  id: "palette",
  label: "cmdPalette",
  group: "view",
  keys: [{ key: "k", ctrl: true }],
  palette: false,
  run: () => {
    patchUi({ paletteOpen: true });
  },
};

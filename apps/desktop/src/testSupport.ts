// Fixtures for component tests. The real `SessionView` mirrors a large Rust struct, so tests
// build only the parts a component reads and cast the rest away.

import type { SessionView } from "./types";

export interface FixtureMarker {
  id: number;
  position_secs: number;
  label: string;
  kind: "Standard" | "ToDo" | "Chapter" | "Highlight";
  completed: boolean;
}

export interface FixtureSequence {
  id: number;
  name: string;
  markers?: FixtureMarker[];
  tracks?: unknown[];
}

/** A session with the given sequences; the first one is active unless `active` says otherwise. */
export function makeSession(sequences: FixtureSequence[], active = 0): SessionView {
  return {
    project: {
      sequences: sequences.map((sequence) => ({
        id: sequence.id,
        name: sequence.name,
        timeline: {
          tracks: sequence.tracks ?? [],
          markers: sequence.markers ?? [],
          playhead_secs: 0,
          multicam_groups: [],
        },
        export_settings: { aspect_ratio: "Original", target_lufs: -14 },
      })),
      active_sequence: active,
      media_library: [],
    },
    filePath: null,
    canUndo: false,
    canRedo: false,
    dirty: false,
  } as unknown as SessionView;
}

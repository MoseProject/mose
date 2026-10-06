import timelineStyles from "../../../src/editor/timeline/timeline.module.css";
import type { Meta, StoryObj } from "@storybook/react-vite";
import { TimelineMinimap } from "../../../src/editor/timeline/TimelineMinimap";
import type { ClipInstance, Track } from "../../../src/types";

const videoClip = {
  start_secs: 0,
  source_in_secs: 0,
  source_out_secs: 22,
  speed_factor: 1,
} as ClipInstance;

const audioClip = {
  start_secs: 18,
  source_in_secs: 4,
  source_out_secs: 35,
  speed_factor: 1,
} as ClipInstance;

const makeTrack = (
  id: number,
  name: string,
  kind: Track["kind"],
  clips: ClipInstance[],
): Track => {
  return {
    id,
    name,
    kind,
    clips,
    text_clips: [],
    shape_clips: [],
    visible: true,
    audio_role: "Unspecified",
    color_label: null,
    locked: false,
    muted: false,
    solo: false,
  };
};

const tracks: Track[] = [
  makeTrack(1, "Gameplay", "Video", [videoClip]),
  makeTrack(2, "Game audio", "Audio", [audioClip]),
];

const meta = {
  title: "Editor/Timeline/Minimap",
  component: TimelineMinimap,
  tags: ["autodocs"],
  args: {
    tracks,
    scrollSecs: 18,
    viewSecs: 24,
    onScrollTo: () => {},
  },
} satisfies Meta<typeof TimelineMinimap>;

export default meta;
type Story = StoryObj<typeof meta>;

export const TimelineOverview: Story = {};

export const LongSequence: Story = {
  args: { scrollSecs: 4200, viewSecs: 1100 },
};

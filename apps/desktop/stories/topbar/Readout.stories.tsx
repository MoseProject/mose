import type { Meta, StoryObj } from "@storybook/react-vite";
import { Readout } from "../../src/topbar/Readout";
import type { SessionView } from "../../src/types";

const session = {
  project: {
    media_library: [
      {
        kind: "Video",
        fps: 60,
        resolution: [1920, 1080],
      },
    ],
  },
} as unknown as SessionView;

const meta = {
  title: "Top bar/Playback readout",
  component: Readout,
  tags: ["autodocs"],
  args: { session },
} satisfies Meta<typeof Readout>;

export default meta;
type Story = StoryObj<typeof meta>;

export const WithSourceDetails: Story = {};

export const TimecodeOnly: Story = {
  args: {
    session: {
      project: { media_library: [] },
    } as unknown as SessionView,
  },
};

import { useState } from "react";
import type { Meta, StoryObj } from "@storybook/react-vite";
import { F32KeyframeList, PositionKeyframeList } from "../../../src/editor/inspector/KeyframeEditor";
import type { Keyframe, Position } from "../../../src/types";

function KeyframeLists() {
  const [opacity, setOpacity] = useState<Keyframe<number>[]>([
    { time_fraction: 0, value: 0 },
    { time_fraction: 0.45, value: 1 },
    { time_fraction: 1, value: 0.75 },
  ]);
  const [position, setPosition] = useState<Keyframe<Position>[]>([
    { time_fraction: 0, value: { x: -0.4, y: 0 } },
    { time_fraction: 1, value: { x: 0.4, y: -0.1 } },
  ]);

  return (
    <div style={{ width: 440, display: "grid", gap: 24 }}>
      <F32KeyframeList
        label="Opacity"
        keyframes={opacity}
        min={0}
        max={1}
        step={0.01}
        defaultValue={1}
        onCommit={setOpacity}
      />
      <PositionKeyframeList
        label="Position"
        keyframes={position}
        onCommit={setPosition}
      />
    </div>
  );
}

const meta = {
  title: "Editor/Inspector/Keyframes",
  component: KeyframeLists,
  tags: ["autodocs"],
} satisfies Meta<typeof KeyframeLists>;

export default meta;
type Story = StoryObj<typeof meta>;

export const AnimatedProperties: Story = {};

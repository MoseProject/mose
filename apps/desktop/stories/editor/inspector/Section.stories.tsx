import type { Meta, StoryObj } from "@storybook/react-vite";
import { Section } from "../../../src/editor/inspector/controls";
import { Slider, NumberField } from "../../../src/editor/inspector/controls";

const meta = {
  title: "Editor/Inspector/Section",
  component: Section,
  tags: ["autodocs"],
  args: {
    label: "Transform",
    defaultOpen: true,
    children: (
      <>
        <Slider
          label="Opacity"
          value={0.72}
          min={0}
          max={1}
          step={0.01}
          format={(value) => `${Math.round(value * 100)}%`}
          onCommit={() => {}}
        />
        <NumberField
          label="Rotation"
          value={12}
          step={1}
          suffix="°"
          onCommit={() => {}}
        />
      </>
    ),
  },
} satisfies Meta<typeof Section>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Expanded: Story = {};

export const Collapsed: Story = {
  args: { defaultOpen: false },
};

export const WithKeyframeAndReset: Story = {
  args: {
    keyframe: { active: true, disabled: false, onToggle: () => {} },
    onReset: () => {},
  },
};

import type { Meta, StoryObj } from "@storybook/react-vite";
import { NumberField, Slider } from "../../../src/editor/inspector/controls";
import { ColorField, SelectField, TextField } from "../../../src/editor/inspector/OverlayFields";

function InspectorFields() {
  return (
    <div style={{ width: 280, display: "grid", gap: 12 }}>
      <TextField
        label="Title"
        value="Match highlights"
        onCommit={() => {}}
      />
      <SelectField
        label="Font"
        value="Inter"
        onChange={() => {}}
      >
        <option>Inter</option>
        <option>Space Grotesk</option>
      </SelectField>
      <ColorField
        label="Text color"
        rgb={[32, 185, 195]}
        onCommit={() => {}}
      />
      <Slider
        label="Opacity"
        value={0.75}
        min={0}
        max={1}
        step={0.01}
        format={(value) => `${Math.round(value * 100)}%`}
        onCommit={() => {}}
      />
      <NumberField
        label="Font size"
        value={36}
        step={1}
        suffix="px"
        onCommit={() => {}}
      />
    </div>
  );
}

const meta = {
  title: "Editor/Inspector/Fields",
  component: InspectorFields,
  tags: ["autodocs"],
} satisfies Meta<typeof InspectorFields>;

export default meta;
type Story = StoryObj<typeof meta>;

export const CommonFields: Story = {};

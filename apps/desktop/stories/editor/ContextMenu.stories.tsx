import type { Meta, StoryObj } from "@storybook/react-vite";
import { ContextMenu } from "../../src/editor/chrome/ContextMenu";

const meta = {
  title: "Editor/Context menu",
  component: ContextMenu,
  tags: ["autodocs"],
  args: {
    x: 80,
    y: 80,
    onClose: () => {},
    items: [
      { label: "Cut", shortcut: "⌘ X", onSelect: () => {} },
      { label: "Copy", shortcut: "⌘ C", onSelect: () => {} },
      { label: "Paste", shortcut: "⌘ V", onSelect: () => {}, separator: true },
      { label: "Delete", disabled: true, onSelect: () => {} },
    ],
  },
} satisfies Meta<typeof ContextMenu>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};

export const NearWindowEdge: Story = {
  args: { x: 9999, y: 9999 },
  parameters: { layout: "fullscreen" },
};

import type { Meta, StoryObj } from "@storybook/react-vite";
import { Icon, type IconName } from "../../src/editor/chrome/Icon";

const iconNames: IconName[] = [
  "star",
  "flag",
  "document",
  "skipBack",
  "skipForward",
  "stepBack",
  "stepForward",
  "play",
  "pause",
  "loop",
  "plus",
  "minus",
  "chevronLeft",
  "chevronRight",
  "chevronDown",
  "box",
  "eye",
  "eyeOff",
  "lock",
  "unlock",
  "more",
  "close",
  "queue",
  "cursor",
  "roll",
  "slip",
  "slide",
  "text",
  "scissors",
  "film",
  "music",
  "fit",
  "search",
  "filter",
  "grid",
  "list",
  "settings",
  "undo",
  "upload",
  "redo",
  "reset",
  "save",
  "folder",
  "gauge",
  "keyframe",
];

function IconGallery() {
  return (
    <div style={{ display: "grid", gridTemplateColumns: "repeat(5, 110px)", gap: 12 }}>
      {iconNames.map((name) => {
        return (
          <div
            key={name}
            style={{ display: "flex", alignItems: "center", gap: 8 }}
          >
            <Icon
              name={name}
              size={18}
            />
            <span>{name}</span>
          </div>
        );
      })}
    </div>
  );
}

const meta = {
  title: "Editor/Icons",
  component: Icon,
  tags: ["autodocs"],
  argTypes: {
    name: { control: "select", options: iconNames },
    filled: { control: "boolean" },
    size: { control: { type: "range", min: 10, max: 32, step: 2 } },
  },
} satisfies Meta<typeof Icon>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Gallery = {
  render: () => {
    return <IconGallery />;
  },
  args: { name: "play" },
  parameters: { layout: "padded" },
} satisfies Story;

export const SingleIcon: Story = {
  args: { name: "play", size: 20, filled: false },
};

import type { Meta, StoryObj } from "@storybook/react-vite";
import { Menu } from "../../src/editor/chrome/Menu";
import { Icon } from "../../src/editor/chrome/Icon";

const meta = {
  title: "Editor/Menu",
  component: Menu,
  tags: ["autodocs"],
  args: {
    label: "Sequence",
    ariaLabel: "Sequence actions",
    children: (
      <>
        <button>Duplicate sequence</button>
        <button>Rename sequence</button>
        <button disabled>Move left</button>
      </>
    ),
  },
} satisfies Meta<typeof Menu>;

export default meta;
type Story = StoryObj<typeof meta>;

export const ToolbarMenu: Story = {};

export const IconMenu: Story = {
  args: {
    label: <Icon name="more" />,
    ariaLabel: "More actions",
  },
};

export const Menubar: Story = {
  args: { variant: "bar" },
};

import exportStyles from "../../src/editor/export/export.module.css";
import { cx } from "../../src/cx";
import baseStyles from "../../src/styles/base.module.css";
import styles from "../../src/editor/chrome/chrome.module.css";
import type { Meta, StoryObj } from "@storybook/react-vite";
import { Dialog } from "../../src/editor/chrome/Dialog";

const meta = {
  title: "Editor/Dialog",
  component: Dialog,
  tags: ["autodocs"],
  args: {
    title: "Create a project",
    onClose: () => {},
    children: (
      <>
        <label className={cx(styles, baseStyles, exportStyles, "dialog-field")}>
          Project name
          <input autoFocus defaultValue="Gameplay highlights" />
        </label>
        <p className={cx(styles, baseStyles, "muted")}>Choose a name that helps you find this edit later.</p>
      </>
    ),
    actions: (
      <>
        <button>Cancel</button>
        <button className={cx(styles, baseStyles, "primary")}>Create project</button>
      </>
    ),
  },
} satisfies Meta<typeof Dialog>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Standard: Story = {};

export const Wide: Story = {
  args: { wide: true, title: "Choose a media folder" },
};

export const RequiredDecision: Story = {
  args: { dismissible: false, title: "Unsaved changes" },
};

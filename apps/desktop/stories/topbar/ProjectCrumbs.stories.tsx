import type { Meta, StoryObj } from "@storybook/react-vite";
import { ProjectCrumbs } from "../../src/topbar/ProjectCrumbs";
import type { SessionView } from "../../src/types";

const session = {
  project: {
    name: "Gameplay highlights",
    active_sequence: 0,
    sequences: [{ id: 1, name: "Main sequence" }],
  },
  dirty: false,
  filePath: null,
} as unknown as SessionView;

const meta = {
  title: "Top bar/Project breadcrumbs",
  component: ProjectCrumbs,
  tags: ["autodocs"],
  args: { session },
} satisfies Meta<typeof ProjectCrumbs>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Saved: Story = {};

export const UnsavedChanges: Story = {
  args: { session: { ...session, dirty: true } },
};

export const SavedToDisk: Story = {
  args: { session: { ...session, filePath: "/Projects/highlights.ocproj" } },
};

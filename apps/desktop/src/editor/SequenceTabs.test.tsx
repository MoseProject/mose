// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { t } from "../i18n";
import { makeSession } from "../testSupport";

vi.mock("../ipc", () => ({
  api: {
    selectSequence: vi.fn().mockResolvedValue(undefined),
    duplicateSequence: vi.fn().mockResolvedValue(undefined),
    renameSequence: vi.fn().mockResolvedValue(undefined),
    removeSequence: vi.fn().mockResolvedValue(undefined),
    moveSequence: vi.fn().mockResolvedValue(undefined),
    insertSequenceAsCompoundClip: vi.fn().mockResolvedValue(true),
    addSequence: vi.fn().mockResolvedValue(undefined),
    addMarker: vi.fn().mockResolvedValue(undefined),
    seek: vi.fn().mockResolvedValue(undefined),
    updateMarker: vi.fn().mockResolvedValue(undefined),
    removeMarker: vi.fn().mockResolvedValue(undefined),
  },
}));

const { SequenceTabs } = await import("./SequenceTabs");
const { api } = await import("../ipc");

const two = () =>
  makeSession([
    { id: 11, name: "Main" },
    { id: 22, name: "Cut B" },
  ]);

beforeEach(() => vi.clearAllMocks());
afterEach(() => cleanup());

describe("sequence tabs", () => {
  it("selects, duplicates and adds sequences", () => {
    render(<SequenceTabs session={two()} />);
    fireEvent.click(screen.getByRole("tab", { name: "Cut B" }));
    expect(api.selectSequence).toHaveBeenCalledWith(1);

    fireEvent.click(screen.getAllByText(t("duplicateSequence"), { selector: "button" })[0]);
    expect(api.duplicateSequence).toHaveBeenCalledWith(0, `Main ${t("copy")}`);

    fireEvent.click(screen.getByTitle(t("addSequence")));
    expect(api.addSequence).toHaveBeenCalledWith(`${t("sequence")} 3`);
  });

  it("marks the active tab and renames through an inline field", () => {
    render(<SequenceTabs session={two()} />);
    expect(screen.getByRole("tab", { name: "Main" }).getAttribute("aria-selected")).toBe("true");
    expect(screen.getByRole("tab", { name: "Cut B" }).getAttribute("aria-selected")).toBe("false");

    fireEvent.click(screen.getAllByText(t("renameSequence"), { selector: "button" })[0]);
    const field = screen.getByLabelText(t("renameSequence")) as HTMLInputElement;
    fireEvent.change(field, { target: { value: "Renamed" } });
    fireEvent.blur(field);
    expect(api.renameSequence).toHaveBeenCalledWith(11, "Renamed");
    expect(screen.queryByLabelText(t("renameSequence"))).toBeNull();
  });

  it("does not rename when the name is unchanged and leaves rename mode on Escape or Enter", () => {
    render(<SequenceTabs session={two()} />);
    fireEvent.click(screen.getAllByText(t("renameSequence"), { selector: "button" })[0]);
    fireEvent.blur(screen.getByLabelText(t("renameSequence")));
    expect(api.renameSequence).not.toHaveBeenCalled();

    fireEvent.click(screen.getAllByText(t("renameSequence"), { selector: "button" })[0]);
    fireEvent.keyDown(screen.getByLabelText(t("renameSequence")), { key: "Escape" });
    expect(screen.queryByLabelText(t("renameSequence"))).toBeNull();

    fireEvent.click(screen.getAllByText(t("renameSequence"), { selector: "button" })[1]);
    const field = screen.getByLabelText(t("renameSequence")) as HTMLInputElement;
    fireEvent.change(field, { target: { value: "Entered" } });
    fireEvent.keyDown(field, { key: "Enter" });
    fireEvent.blur(field);
    expect(api.renameSequence).toHaveBeenCalledWith(22, "Entered");
  });

  it("removes and reorders, with the buttons disabled where the action makes no sense", () => {
    render(<SequenceTabs session={two()} />);
    const remove = screen.getAllByText(t("removeSequence"), { selector: "button" });
    fireEvent.click(remove[1]);
    expect(api.removeSequence).toHaveBeenCalledWith(22);

    const left = screen.getAllByText(t("moveSequenceLeft"), { selector: "button" });
    const right = screen.getAllByText(t("moveSequenceRight"), { selector: "button" });
    expect((left[0] as HTMLButtonElement).disabled).toBe(true);
    expect((right[1] as HTMLButtonElement).disabled).toBe(true);
    fireEvent.click(right[0]);
    expect(api.moveSequence).toHaveBeenCalledWith(0, 1);
    fireEvent.click(left[1]);
    expect(api.moveSequence).toHaveBeenCalledWith(1, 0);

    // The active sequence cannot be nested into itself.
    const insert = screen.getAllByText(t("insertAsCompoundClip"), { selector: "button" });
    expect((insert[0] as HTMLButtonElement).disabled).toBe(true);
    fireEvent.click(insert[1]);
    expect(api.insertSequenceAsCompoundClip).toHaveBeenCalledWith(22);
  });

  it("will not remove the only sequence", () => {
    render(<SequenceTabs session={makeSession([{ id: 1, name: "Only" }])} />);
    expect((screen.getByText(t("removeSequence"), { selector: "button" }) as HTMLButtonElement).disabled).toBe(true);
  });
});

describe("marker list", () => {
  const session = () =>
    makeSession([
      {
        id: 1,
        name: "Main",
        markers: [
          { id: 2, position_secs: 9, label: "late", kind: "Chapter", completed: false },
          { id: 1, position_secs: 3.5, label: "early", kind: "ToDo", completed: true },
        ],
      },
    ]);

  it("lists markers in time order and adds a new one", () => {
    render(<SequenceTabs session={session()} />);
    const labels = screen.getAllByLabelText(t("markerLabel")) as HTMLInputElement[];
    expect(labels.map((input) => input.value)).toEqual(["early", "late"]);
    fireEvent.click(screen.getByText(t("addMarker")));
    expect(api.addMarker).toHaveBeenCalledWith();
  });

  it("seeks to a marker and removes it", () => {
    render(<SequenceTabs session={session()} />);
    fireEvent.click(screen.getAllByTitle(t("seekMarker"))[0]);
    expect(api.seek).toHaveBeenCalledWith(3.5);
    fireEvent.click(screen.getAllByTitle(t("removeMarker"))[1]);
    expect(api.removeMarker).toHaveBeenCalledWith(2);
  });

  it("edits a label only when it changed, and the kind and the done flag of a to-do", () => {
    render(<SequenceTabs session={session()} />);
    const [early] = screen.getAllByLabelText(t("markerLabel")) as HTMLInputElement[];
    fireEvent.blur(early);
    expect(api.updateMarker).not.toHaveBeenCalled();
    fireEvent.change(early, { target: { value: "renamed" } });
    fireEvent.blur(early);
    expect(api.updateMarker).toHaveBeenCalledWith(1, { label: "renamed" });

    const [kind] = screen.getAllByLabelText(t("markerKind")) as HTMLSelectElement[];
    fireEvent.change(kind, { target: { value: "Highlight" } });
    expect(api.updateMarker).toHaveBeenCalledWith(1, { kind: "Highlight" });

    // Only a to-do marker has a completion checkbox.
    const done = screen.getByLabelText(t("markerCompleted")) as HTMLInputElement;
    expect(screen.getAllByLabelText(t("markerCompleted"))).toHaveLength(1);
    expect(done.checked).toBe(true);
    fireEvent.click(done);
    expect(api.updateMarker).toHaveBeenCalledWith(1, { completed: false });
  });
});

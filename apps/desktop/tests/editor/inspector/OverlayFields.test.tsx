// @vitest-environment jsdom
import {
  act,
  cleanup,
  fireEvent,
  render,
  renderHook,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { OverlayCatalog } from "../../../src/types";

vi.mock("../../../src/ipc", () => {
  return {
    api: { overlayCatalog: vi.fn() },
  };
});

const { ColorField, SelectField, TextField, useOverlayCatalog } =
  await import("../../../src/editor/inspector/OverlayFields");
const { api } = await import("../../../src/ipc");

beforeEach(() => {
  return vi.clearAllMocks();
});
afterEach(() => {
  return cleanup();
});

describe("overlay catalog hook", () => {
  const catalog = { fonts: [], shapes: [] } as unknown as OverlayCatalog;

  it("retries after a failed load, then fetches once and serves the cache", async () => {
    vi.mocked(api.overlayCatalog).mockRejectedValueOnce(new Error("offline"));
    const first = renderHook(() => {
      return useOverlayCatalog();
    });
    expect(first.result.current).toBeNull();
    await waitFor(() => {
      return expect(api.overlayCatalog).toHaveBeenCalledTimes(1);
    });
    first.unmount();

    vi.mocked(api.overlayCatalog).mockResolvedValue(catalog);
    const second = renderHook(() => {
      return useOverlayCatalog();
    });
    await waitFor(() => {
      return expect(second.result.current).toBe(catalog);
    });
    expect(api.overlayCatalog).toHaveBeenCalledTimes(2);

    const third = renderHook(() => {
      return useOverlayCatalog();
    });
    expect(third.result.current).toBe(catalog);
    expect(api.overlayCatalog).toHaveBeenCalledTimes(2);
  });
});

describe("TextField", () => {
  it("commits on blur only when the text changed", () => {
    const onCommit = vi.fn();
    render(
      <TextField
        label="Name"
        value="abc"
        placeholder="type"
        onCommit={onCommit}
      />,
    );
    const input = screen.getByPlaceholderText("type") as HTMLInputElement;

    fireEvent.blur(input);
    expect(onCommit).not.toHaveBeenCalled();
    fireEvent.change(input, { target: { value: "abcd" } });
    fireEvent.blur(input);
    expect(onCommit).toHaveBeenCalledWith("abcd");
  });

  it("commits on Enter, reverts on Escape and follows the parent's value", () => {
    const onCommit = vi.fn();
    const { rerender } = render(
      <TextField
        label="Name"
        value="abc"
        onCommit={onCommit}
      />,
    );
    const input = screen.getByRole("textbox") as HTMLInputElement;
    input.focus();
    fireEvent.change(input, { target: { value: "xyz" } });
    fireEvent.keyDown(input, { key: "Enter" });
    expect(onCommit).toHaveBeenCalledWith("xyz");

    fireEvent.change(input, { target: { value: "typo" } });
    fireEvent.keyDown(input, { key: "Escape" });
    expect(input.value).toBe("abc");

    rerender(
      <TextField
        label="Name"
        value="from-parent"
        onCommit={onCommit}
      />,
    );
    expect(input.value).toBe("from-parent");
  });
});

describe("SelectField", () => {
  it("reports the chosen value and honours disabled", () => {
    const onChange = vi.fn();
    render(
      <SelectField
        label="Font"
        value="a"
        onChange={onChange}
      >
        <option value="a">A</option>
        <option value="b">B</option>
      </SelectField>,
    );
    fireEvent.change(screen.getByRole("combobox"), { target: { value: "b" } });
    expect(onChange).toHaveBeenCalledWith("b");
    cleanup();

    render(
      <SelectField
        label="Font"
        value="a"
        disabled
        onChange={onChange}
      >
        <option value="a">A</option>
      </SelectField>,
    );
    expect((screen.getByRole("combobox") as HTMLSelectElement).disabled).toBe(true);
  });
});

describe("ColorField", () => {
  const colorInput = (container: HTMLElement) => {
    return container.querySelector('input[type="color"]') as HTMLInputElement;
  };

  it("shows the current color and commits a new one as rgb", () => {
    const onCommit = vi.fn();
    const { container } = render(
      <ColorField
        label="Fill"
        rgb={[255, 0, 0]}
        onCommit={onCommit}
      />,
    );
    expect(colorInput(container).value).toBe("#ff0000");

    fireEvent.change(colorInput(container), { target: { value: "#00ff80" } });
    expect(onCommit).toHaveBeenCalledWith([0, 255, 128]);
  });

  it("does not commit the unchanged color and follows the parent", () => {
    const onCommit = vi.fn();
    const { container, rerender } = render(
      <ColorField
        label="Fill"
        rgb={[255, 0, 0]}
        onCommit={onCommit}
      />,
    );
    fireEvent.change(colorInput(container), { target: { value: "#FF0000" } });
    expect(onCommit).not.toHaveBeenCalled();

    act(() => {
      return rerender(
        <ColorField
          label="Fill"
          rgb={[0, 0, 255]}
          onCommit={onCommit}
        />,
      );
    });
    expect(colorInput(container).value).toBe("#0000ff");
  });
});

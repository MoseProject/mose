// @vitest-environment jsdom
import { StrictMode } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";

const render = vi.fn();
const createRoot = vi.fn(() => {
  return { render };
});
vi.mock("react-dom/client", () => {
  return { createRoot };
});
vi.mock("./App", () => {
  return {
    App: () => {
      return null;
    },
  };
});

afterEach(() => {
  document.body.innerHTML = "";
  vi.clearAllMocks();
  vi.resetModules();
});

describe("entry point", () => {
  it("mounts the app in strict mode on the root element", async () => {
    document.body.innerHTML = '<div id="root"></div>';
    await import("./main");
    expect(createRoot).toHaveBeenCalledWith(document.getElementById("root"));
    expect(render).toHaveBeenCalledTimes(1);
    const tree = render.mock.calls[0][0] as { type: unknown };
    expect(tree.type).toBe(StrictMode);
  });
});

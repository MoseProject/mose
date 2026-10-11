// @vitest-environment jsdom
import { beforeEach, describe, expect, it, vi } from "vitest";

const splitAtPlayhead = vi.fn().mockResolvedValue(undefined);
const controlReply = vi.fn().mockResolvedValue(undefined);
let requestHandler: ((request: unknown) => void) | undefined;

vi.mock("../src/ipc", () => {
  return {
    api: {
      splitAtPlayhead: (...args: unknown[]) => {
        return splitAtPlayhead(...args);
      },
      controlReply: (...args: unknown[]) => {
        return controlReply(...args);
      },
    },
    onControlRequest: (handler: (request: unknown) => void) => {
      requestHandler = handler;
      return Promise.resolve(() => {});
    },
  };
});

const { handleControlRequest, listCommands, listenForControlRequests } =
  await import("../src/control");
const { COMMANDS } = await import("../src/commands");

beforeEach(() => {
  vi.clearAllMocks();
  requestHandler = undefined;
});

describe("control channel, window side", () => {
  it("lists every command of the table with its label, group and bindings", () => {
    const list = listCommands();
    expect(list).toHaveLength(COMMANDS.length);
    const split = list.find((command) => {
      return command.id === "split";
    });
    expect(split).toMatchObject({ id: "split", keys: ["S", "C"] });
    expect(split?.label).not.toBe("");
    expect(split?.group).not.toBe("");
  });

  it("runs a command by id the way a key press would", async () => {
    await expect(handleControlRequest("commands.run", { id: "split" })).resolves.toBeNull();
    expect(splitAtPlayhead).toHaveBeenCalledTimes(1);
  });

  it("refuses an unknown command and a missing id", async () => {
    await expect(handleControlRequest("commands.run", { id: "nope" })).rejects.toThrow(
      "unknown command: nope",
    );
    await expect(handleControlRequest("commands.run", {})).rejects.toThrow("unknown command");
    expect(splitAtPlayhead).not.toHaveBeenCalled();
  });

  it("refuses a command that is disabled right now", async () => {
    const command = COMMANDS.find((candidate) => {
      return candidate.enabled !== undefined;
    });
    expect(command).toBeDefined();
    const target = command!;
    const original = target.enabled;
    target.enabled = () => {
      return false;
    };
    const run = vi.spyOn(target, "run");
    await expect(handleControlRequest("commands.run", { id: target.id })).rejects.toThrow(
      "command is disabled",
    );
    expect(run).not.toHaveBeenCalled();
    target.enabled = original;
  });

  it("offers no way to call a backend command directly", async () => {
    for (const method of ["ipc.invoke", "invoke", "session.get"]) {
      await expect(
        handleControlRequest(method, { command: "start_export", args: {} }),
      ).rejects.toThrow("unknown method: " + method);
    }
  });

  it("answers a forwarded request with its result, or with the error as text", async () => {
    await listenForControlRequests();
    requestHandler?.({ requestId: 4, method: "commands.run", params: { id: "split" } });
    await vi.waitFor(() => {
      return expect(controlReply).toHaveBeenCalledWith(4, true, null);
    });
    requestHandler?.({ requestId: 5, method: "commands.run", params: { id: "nope" } });
    await vi.waitFor(() => {
      return expect(controlReply).toHaveBeenCalledWith(5, false, expect.stringContaining("nope"));
    });
  });
});

// The window's half of the control channel (src-tauri/src/control.rs): answers the requests the
// backend forwards, so something outside the window can list and run the same commands the
// keyboard, the palette and the menus use.
//
//   commands.list                      every command with its label, group, bindings and state
//   commands.run    { id }             run one, as a key press would (refused when disabled)
//
// Only commands of the table can be run: the same ones the keyboard and the palette offer, and
// nothing that reaches the Rust side directly.

import { COMMANDS, commandLabel, findCommand, keysOf } from "./commands";
import { GROUP_LABELS } from "./commands/types";
import { formatKeys } from "./shortcuts";
import { api, onControlRequest, type ControlRequest } from "./ipc";
import { t } from "./i18n";
import { describeError } from "./store";

export interface CommandInfo {
  id: string;
  label: string;
  group: string;
  keys: string[];
  enabled: boolean;
}

function asRecord(value: unknown): Record<string, unknown> {
  return typeof value === "object" && value !== null ? (value as Record<string, unknown>) : {};
}

/** Every command as plain data, in the order of the palette. */
export function listCommands(): CommandInfo[] {
  return COMMANDS.map((command) => {
    return {
      id: command.id,
      label: commandLabel(command),
      group: t(GROUP_LABELS[command.group]),
      keys: keysOf(command).map(formatKeys),
      enabled: command.enabled ? command.enabled() : true,
    };
  });
}

/** Answers one request; a rejection is the error sent back. */
export async function handleControlRequest(method: string, params: unknown): Promise<unknown> {
  const args = asRecord(params);
  switch (method) {
    case "commands.list": {
      return listCommands();
    }
    case "commands.run": {
      const id = args.id;
      const command = typeof id === "string" ? findCommand(id) : undefined;
      if (!command) {
        throw new Error("unknown command: " + String(id));
      }
      if (command.enabled && !command.enabled()) {
        throw new Error("command is disabled: " + command.id);
      }
      await command.run();
      return null;
    }
    default: {
      throw new Error("unknown method: " + method);
    }
  }
}

async function answer(request: ControlRequest) {
  try {
    const value = await handleControlRequest(request.method, request.params);
    await api.controlReply(request.requestId, true, value ?? null);
  } catch (error) {
    await api.controlReply(request.requestId, false, describeError(error));
  }
}

/** Starts answering forwarded requests, then tells the backend the window is listening (a
 *  request sent before that would be lost). Nothing is sent unless the backend runs the channel. */
export async function listenForControlRequests() {
  const unlisten = await onControlRequest((request) => {
    void answer(request);
  });
  await api.controlReady();
  return unlisten;
}

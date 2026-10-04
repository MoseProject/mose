// @vitest-environment jsdom
import { describe, expect, it } from "vitest";
import { isActivatableTarget, isEditableTarget, shouldIgnoreShortcut } from "./keyboard";

const element = <K extends keyof HTMLElementTagNameMap>(tag: K) => {
  return document.createElement(tag);
};

describe("isEditableTarget", () => {
  it("is true for typed-input controls", () => {
    for (const tag of ["input", "textarea", "select"] as const) {
      expect(isEditableTarget(element(tag))).toBe(true);
    }
  });

  it("is true for contenteditable elements and false for plain ones", () => {
    const editable = element("div");
    editable.contentEditable = "true";
    Object.defineProperty(editable, "isContentEditable", { value: true });
    expect(isEditableTarget(editable)).toBe(true);
    expect(isEditableTarget(element("div"))).toBe(false);
    expect(isEditableTarget(element("button"))).toBe(false);
    expect(isEditableTarget(null)).toBe(false);
  });
});

describe("isActivatableTarget", () => {
  it("covers buttons, links, summaries and button-like roles", () => {
    expect(isActivatableTarget(element("button"))).toBe(true);
    expect(isActivatableTarget(element("a"))).toBe(true);
    const summary = document.createElement("summary");
    expect(isActivatableTarget(summary)).toBe(true);
    for (const role of ["button", "tab", "menuitem"]) {
      const div = element("div");
      div.setAttribute("role", role);
      expect(isActivatableTarget(div)).toBe(true);
    }
  });

  it("is false for the page body and non-interactive elements", () => {
    expect(isActivatableTarget(document.body)).toBe(false);
    expect(isActivatableTarget(element("div"))).toBe(false);
    expect(isActivatableTarget(null)).toBe(false);
  });
});

describe("shouldIgnoreShortcut", () => {
  const event = (target: EventTarget) => {
    const e = new KeyboardEvent("keydown", { key: "s" });
    Object.defineProperty(e, "target", { value: target });
    return e;
  };

  it("stands down while a dialog is open", () => {
    expect(shouldIgnoreShortcut(event(document.body), 1)).toBe(true);
    expect(shouldIgnoreShortcut(event(document.body), 0)).toBe(false);
  });

  it("stands down inside editable fields", () => {
    expect(shouldIgnoreShortcut(event(element("textarea")), 0)).toBe(true);
  });
});

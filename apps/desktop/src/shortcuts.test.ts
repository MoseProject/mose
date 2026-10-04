// @vitest-environment jsdom
import { describe, expect, it } from "vitest";
import { formatKeys, matchesKeys } from "./shortcuts";

const press = (init: KeyboardEventInit) => {
  return new KeyboardEvent("keydown", init);
};

describe("matchesKeys", () => {
  it("matches a plain key regardless of letter case", () => {
    expect(matchesKeys(press({ key: "s" }), { key: "s" })).toBe(true);
    expect(matchesKeys(press({ key: "S", shiftKey: true }), { key: "s", shift: true })).toBe(true);
  });

  it("requires the modifiers to match exactly", () => {
    expect(matchesKeys(press({ key: "z", ctrlKey: true }), { key: "z", ctrl: true })).toBe(true);
    expect(matchesKeys(press({ key: "z" }), { key: "z", ctrl: true })).toBe(false);
    expect(matchesKeys(press({ key: "z", ctrlKey: true }), { key: "z" })).toBe(false);
    expect(matchesKeys(press({ key: "m", altKey: true }), { key: "m" })).toBe(false);
    expect(matchesKeys(press({ key: "m", shiftKey: true }), { key: "m" })).toBe(false);
  });

  it("counts Meta as Ctrl", () => {
    expect(matchesKeys(press({ key: "s", metaKey: true }), { key: "s", ctrl: true })).toBe(true);
  });

  it("lets anyShift ignore Shift", () => {
    expect(matchesKeys(press({ key: "S", shiftKey: true }), { key: "s", anyShift: true })).toBe(
      true,
    );
    expect(matchesKeys(press({ key: "s" }), { key: "s", anyShift: true })).toBe(true);
  });

  it("matches by code for layout-dependent keys", () => {
    expect(matchesKeys(press({ key: " ", code: "Space" }), { code: "Space" })).toBe(true);
    expect(matchesKeys(press({ key: "a", code: "KeyA" }), { code: "Space" })).toBe(false);
  });

  it("never matches a spec with neither key nor code", () => {
    expect(matchesKeys(press({ key: "a" }), {})).toBe(false);
  });
});

describe("formatKeys", () => {
  it("writes modifiers in a fixed order and upper-cases letters", () => {
    expect(formatKeys({ key: "z", ctrl: true, shift: true })).toBe("Ctrl+Shift+Z");
    expect(formatKeys({ key: "m", alt: true })).toBe("Alt+M");
  });

  it("names the special keys", () => {
    expect(formatKeys({ code: "Space" })).toBe("Space");
    expect(formatKeys({ key: "ArrowLeft", shift: true })).toBe("Shift+←");
    expect(formatKeys({ key: "Delete" })).toBe("Del");
    expect(formatKeys({ key: "Escape" })).toBe("Esc");
    expect(formatKeys({ key: "F2" })).toBe("F2");
  });
});

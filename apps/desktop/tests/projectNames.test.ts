import { describe, expect, it } from "vitest";
import { funnyProjectName, uniqueFunnyName } from "../src/projectNames";

/** A generator that walks through fixed numbers, so the names are predictable. */
const sequence = (...values: number[]) => {
  let index = 0;
  return () => {
    const value = values[index % values.length];
    index += 1;
    return value;
  };
};

describe("funnyProjectName", () => {
  it("builds a booster and a thing for the short shape", () => {
    expect(funnyProjectName(sequence(0, 0.1, 0), "en")).toBe("Mega Studio");
  });

  it("adds a title for the medium shape", () => {
    expect(funnyProjectName(sequence(0, 0.5, 0, 0), "en")).toBe("Mega Studio Master");
  });

  it("puts a second booster before the title for the long shape", () => {
    expect(funnyProjectName(sequence(0.32, 0.9, 0.1, 0), "en")).toBe("Imax Giga Master");
  });

  it("never repeats the booster in the long shape", () => {
    // Both boosters land on the same entry, so the second one moves to the next.
    expect(funnyProjectName(sequence(0, 0.9, 0, 0), "en")).toBe("Mega Giga Master");
  });

  it("speaks Portuguese when asked to", () => {
    expect(funnyProjectName(sequence(0, 0.1, 0), "pt-BR")).toBe("Mega Estúdio");
  });

  it("stays inside the lists whatever the generator returns", () => {
    for (const edge of [0, 0.999999, 1, 1.5]) {
      const name = funnyProjectName(() => {
        return edge;
      }, "en");
      expect(name).not.toMatch(/undefined/);
      expect(name.split(" ").length).toBeGreaterThanOrEqual(2);
    }
  });

  it("gives a variety of names with the real random source", () => {
    const names = new Set<string>();
    for (let i = 0; i < 200; i += 1) {
      names.add(funnyProjectName(Math.random, "en"));
    }
    expect(names.size).toBeGreaterThan(50);
  });
});

describe("uniqueFunnyName", () => {
  it("skips a name that is already taken, regardless of case and spaces", () => {
    const name = uniqueFunnyName(["  mega studio "], sequence(0, 0.1, 0, 0.1, 0.1, 0.5), "en");
    expect(name).not.toBe("Mega Studio");
    expect(name).toBe("Giga Clip");
  });

  it("falls back to a numbered name when every draw is taken", () => {
    const name = uniqueFunnyName(["Mega Studio"], sequence(0, 0.1, 0), "en");
    expect(name).toBe("Mega Studio 2");
  });

  it("counts past a numbered name that is also taken", () => {
    const name = uniqueFunnyName(["Mega Studio", "Mega Studio 2"], sequence(0, 0.1, 0), "en");
    expect(name).toBe("Mega Studio 3");
  });

  it("returns the first draw when nothing is taken", () => {
    expect(uniqueFunnyName([], sequence(0, 0.1, 0), "en")).toBe("Mega Studio");
  });
});

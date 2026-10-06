// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import {
  formatTime,
  formatTimecode,
  localeFromPrefs,
  localeSnapshot,
  setLocale,
  subscribeLocale,
  t,
  type TextKey,
} from "../src/i18n";

afterEach(() => {
  return setLocale("pt-BR");
});

describe("locale", () => {
  it("maps stored preferences to a supported locale, defaulting to Portuguese", () => {
    expect(localeFromPrefs("en")).toBe("en");
    expect(localeFromPrefs("pt-BR")).toBe("pt-BR");
    expect(localeFromPrefs("de")).toBe("pt-BR");
    expect(localeFromPrefs("")).toBe("pt-BR");
  });

  it("switches the language of t() and tells subscribers", () => {
    const listener = vi.fn();
    const unsubscribe = subscribeLocale(listener);
    setLocale("en");
    expect(localeSnapshot()).toBe("en");
    expect(t("save")).toBe("Save");
    setLocale("pt-BR");
    expect(t("save")).toBe("Salvar");
    expect(listener).toHaveBeenCalledTimes(2);

    unsubscribe();
    setLocale("en");
    expect(listener).toHaveBeenCalledTimes(2);
  });

  it("has a non-empty string in both languages for every key", async () => {
    // The table is private, so walk the keys through the source of truth: the module's own
    // text. Reading the file keeps this test honest about newly added keys.
    const { readFileSync } = await import("node:fs");
    const { resolve } = await import("node:path");
    const source = readFileSync(resolve(__dirname, "i18n/locales/en.ts"), "utf8");
    const keys = [...source.matchAll(/^ {2}"?(\w+)"?:/gm)].map((match) => {
      return match[1] as TextKey;
    });
    expect(keys.length).toBeGreaterThan(100);
    for (const locale of ["pt-BR", "en"] as const) {
      setLocale(locale);
      for (const key of keys) {
        expect(t(key), `${locale}:${key}`).not.toEqual("");
      }
    }
  });
});

describe("formatTime", () => {
  it("formats minutes, seconds and hundredths", () => {
    expect(formatTime(0)).toBe("00:00.00");
    expect(formatTime(1.5)).toBe("00:01.50");
    expect(formatTime(61.25)).toBe("01:01.25");
    expect(formatTime(599.994)).toBe("09:59.99");
  });

  it("clamps negative times to zero and rounds to the nearest hundredth", () => {
    expect(formatTime(-3)).toBe("00:00.00");
    expect(formatTime(0.004)).toBe("00:00.00");
    expect(formatTime(0.006)).toBe("00:00.01");
  });
});

describe("formatTimecode", () => {
  it("writes hours, minutes, seconds and frames", () => {
    expect(formatTimecode(0, 24)).toBe("00:00:00:00");
    expect(formatTimecode(134 + 8 / 24, 24)).toBe("00:02:14:08");
    expect(formatTimecode(3661.5, 30)).toBe("01:01:01:15");
  });

  it("rounds a fractional frame rate to whole frames per second", () => {
    expect(formatTimecode(1, 23.976)).toBe("00:00:01:00");
    expect(formatTimecode(1.5, 59.94)).toBe("00:00:01:30");
  });

  it("falls back to 30 fps and clamps negatives", () => {
    expect(formatTimecode(1.5, 0)).toBe("00:00:01:15");
    expect(formatTimecode(-3, 24)).toBe("00:00:00:00");
  });

  it("does not wrap past two digits of hours", () => {
    expect(formatTimecode(100 * 3600, 25)).toBe("100:00:00:00");
  });
});

describe("the document language", () => {
  it("follows the interface language", () => {
    setLocale("en");
    expect(document.documentElement.lang).toBe("en");
    setLocale("pt-BR");
    expect(document.documentElement.lang).toBe("pt-BR");
  });
});

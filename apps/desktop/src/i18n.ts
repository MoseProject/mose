import { useMemo, useSyncExternalStore } from "react";
import { en } from "./i18n/locales/en";
import { ptBR } from "./i18n/locales/pt-BR";

export type Locale = "pt-BR" | "en";
export type TextKey = keyof typeof en;

const translations: Record<Locale, Record<TextKey, string>> = { "pt-BR": ptBR, en };




export function detectLocale(): Locale {
  return navigator.language.toLowerCase().startsWith("pt") ? "pt-BR" : "en";
}

let current: Locale = typeof navigator === "undefined" ? "en" : detectLocale();
const localeListeners = new Set<() => void>();

export function setLocale(locale: Locale) {
  if (current === locale) {
    return;
  }
  current = locale;
  if (typeof document !== "undefined") {
    document.documentElement.lang = locale;
  }
  localeListeners.forEach((listener) => {
    return listener();
  });
}

export function localeFromPrefs(locale: string): Locale {
  return locale === "en" ? "en" : "pt-BR";
}

export function subscribeLocale(listener: () => void) {
  localeListeners.add(listener);
  return () => {
    return localeListeners.delete(listener);
  };
}

export function localeSnapshot() {
  return current;
}


export function gettext(key: TextKey): string {
  return translations[current][key];
}

export const t = gettext;

export interface I18n {
  locale: Locale;
  setLocale: typeof setLocale;
  gettext: typeof gettext;
  t: typeof gettext;
  formatTime: typeof formatTime;
  formatTimecode: typeof formatTimecode;
  localeFromPrefs: typeof localeFromPrefs;
}

export function useI18n(): I18n {
  const locale = useSyncExternalStore(subscribeLocale, localeSnapshot, localeSnapshot);
  return useMemo(() => {
    return { locale, setLocale, gettext, t: gettext, formatTime, formatTimecode, localeFromPrefs };
  }, [locale]);
}

/** `mm:ss.hh` for timeline/transport readouts (hundredths, not frames: fps varies per clip). */
/** `HH:MM:SS:FF` at `fps` frames per second (the usual editing timecode). */
export function formatTimecode(secs: number, fps: number): string {
  const rate = fps > 0 ? Math.round(fps) : 30;
  const totalFrames = Math.round(Math.max(0, secs) * rate);
  const frames = totalFrames % rate;
  const totalSecs = Math.floor(totalFrames / rate);
  const pad = (n: number) => {
    return n.toString().padStart(2, "0");
  };
  return [
    pad(Math.floor(totalSecs / 3600)),
    pad(Math.floor((totalSecs % 3600) / 60)),
    pad(totalSecs % 60),
    pad(frames),
  ].join(":");
}

export function formatTime(secs: number): string {
  const totalHundredths = Math.round(Math.max(0, secs) * 100);
  const minutes = Math.floor(totalHundredths / 6000);
  const seconds = Math.floor((totalHundredths % 6000) / 100);
  const hundredths = totalHundredths % 100;
  const pad = (n: number) => {
    return n.toString().padStart(2, "0");
  };
  return `${pad(minutes)}:${pad(seconds)}.${pad(hundredths)}`;
}

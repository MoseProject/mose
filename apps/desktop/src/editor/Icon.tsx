// Stroke icons (24px grid, 2px stroke, round caps) so every glyph in the editor chrome shares
// one weight and renders the same on every platform, unlike emoji or text symbols.

const PATHS = {
  star: "M12 3l2.8 5.7 6.2.9-4.5 4.4 1.1 6.2L12 17.2 6.4 20.2l1.1-6.2L3 9.6l6.2-.9z",
  document: "M6 3h8l4 4v14H6zM14 3v4h4M9 12h6M9 16h6",
  skipBack: "M7 5v14M19 5l-9 7 9 7z",
  skipForward: "M17 5v14M5 5l9 7-9 7z",
  play: "M7 4l13 8-13 8z",
  pause: "M8 5v14M16 5v14",
  plus: "M12 5v14M5 12h14",
  minus: "M5 12h14",
  chevronLeft: "M15 5l-7 7 7 7",
  chevronRight: "M9 5l7 7-7 7",
  box: "M3 7l9-4 9 4v10l-9 4-9-4zM3 7l9 4 9-4M12 11v10",
  eye: "M2 12s4-7 10-7 10 7 10 7-4 7-10 7S2 12 2 12zM12 9a3 3 0 100 6 3 3 0 000-6z",
  eyeOff: "M3 3l18 18M10.6 5.1A9.7 9.7 0 0112 5c6 0 10 7 10 7a17 17 0 01-3.2 4M6.5 6.6C3.7 8.3 2 12 2 12s4 7 10 7c1.6 0 3-.4 4.3-1M9.9 9.9a3 3 0 004.2 4.2",
  lock: "M6 11h12v9H6zM8 11V8a4 4 0 018 0v3",
  unlock: "M6 11h12v9H6zM8 11V8a4 4 0 017.5-2",
  more: "M5 12h.01M12 12h.01M19 12h.01",
  close: "M6 6l12 12M18 6L6 18",
  queue: "M4 6h16M4 12h16M4 18h10",
} as const;

export type IconName = keyof typeof PATHS;

export function Icon({ name, filled = false, size = 14 }: { name: IconName; filled?: boolean; size?: number }) {
  return (
    <svg
      className="glyph"
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill={filled ? "currentColor" : "none"}
      stroke="currentColor"
      strokeWidth={2}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <path d={PATHS[name]} />
    </svg>
  );
}

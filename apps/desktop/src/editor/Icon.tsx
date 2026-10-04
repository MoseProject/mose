// Every icon in the app comes from lucide (https://lucide.dev), so the chrome shares one stroke
// weight and style. Components ask for icons by role (`play`, `lock`) rather than importing
// lucide directly, which keeps the choice of glyph in one place.

import {
  ArrowRightLeft,
  Box,
  ChevronDown,
  ChevronLeft,
  ChevronRight,
  ChevronsLeft,
  ChevronsLeftRight,
  ChevronsRight,
  Ellipsis,
  Eye,
  Flag,
  EyeOff,
  Film,
  FileText,
  Folder,
  Gauge,
  LayoutGrid,
  List,
  ListFilter,
  ListVideo,
  Lock,
  LockOpen,
  Maximize,
  Minus,
  MousePointer2,
  Music,
  Pause,
  Play,
  Plus,
  Redo2,
  Repeat,
  Save,
  Scissors,
  Search,
  Settings,
  SkipBack,
  SkipForward,
  Star,
  Type,
  Undo2,
  Upload,
  UnfoldHorizontal,
  X,
  type LucideIcon,
} from "lucide-react";

const ICONS = {
  star: Star,
  flag: Flag,
  document: FileText,
  skipBack: SkipBack,
  skipForward: SkipForward,
  stepBack: ChevronsLeft,
  stepForward: ChevronsRight,
  play: Play,
  pause: Pause,
  loop: Repeat,
  plus: Plus,
  minus: Minus,
  chevronLeft: ChevronLeft,
  chevronRight: ChevronRight,
  chevronDown: ChevronDown,
  box: Box,
  eye: Eye,
  eyeOff: EyeOff,
  lock: Lock,
  unlock: LockOpen,
  more: Ellipsis,
  close: X,
  queue: ListVideo,
  cursor: MousePointer2,
  roll: ChevronsLeftRight,
  slip: UnfoldHorizontal,
  slide: ArrowRightLeft,
  text: Type,
  scissors: Scissors,
  film: Film,
  music: Music,
  fit: Maximize,
  search: Search,
  filter: ListFilter,
  grid: LayoutGrid,
  list: List,
  settings: Settings,
  undo: Undo2,
  upload: Upload,
  redo: Redo2,
  save: Save,
  folder: Folder,
  gauge: Gauge,
} satisfies Record<string, LucideIcon>;

export type IconName = keyof typeof ICONS;

export function Icon({ name, filled = false, size = 14 }: { name: IconName; filled?: boolean; size?: number }) {
  const Glyph = ICONS[name];
  return (
    <Glyph
      className="glyph"
      size={size}
      strokeWidth={2}
      fill={filled ? "currentColor" : "none"}
      aria-hidden="true"
    />
  );
}

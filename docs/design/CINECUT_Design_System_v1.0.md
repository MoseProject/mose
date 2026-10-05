# oca Design System

**Version:** 1.0\
**Status:** Foundation\
**Product:** oca --- Professional Video Editor\
**Accent:** Petroleum Blue

------------------------------------------------------------------------

## 1. Design Philosophy

### Core principle

> **Familiar structure. Distinctive execution.**

oca should not reinvent the established workflow of professional
video editors. It should make that workflow feel more precise, cohesive,
and unmistakably oca.

The interface should feel like a **professional creative workstation**,
not a SaaS dashboard or an AI product.

### Visual keywords

-   Professional
-   Precise
-   Technical
-   Cinematic
-   Dense
-   Quiet
-   Functional
-   Contextual
-   Keyboard-friendly

### Avoid

-   Glassmorphism
-   Excessive gradients
-   Neon effects
-   Excessive glow
-   Giant rounded cards
-   Excessive whitespace
-   Pills everywhere
-   Generic SaaS dashboard patterns
-   "AI" visual language
-   Decorative UI with no functional purpose
-   Excessive animation

------------------------------------------------------------------------

## 2. Brand Concept

### CINE

Represents:

-   Frame
-   Cinema
-   Camera
-   Timeline
-   Timecode
-   Composition
-   Image

### CUT

Represents:

-   Precision
-   Division
-   Selection
-   Editing
-   Transition
-   Intervention

> **Everything in oca should subtly reference frames, cuts, and
> timelines.**

Express this through precise borders, timeline divisions, technical
typography, playheads, keyframes, frame-based thumbnails, and structured
spacing.

------------------------------------------------------------------------

## 3. Color System

### Base colors

  Token           Value       Usage
  --------------- ----------- ------------------------------------
  `bg.canvas`     `#090A0D`   Application canvas
  `bg.panel`      `#101216`   Main panels
  `bg.elevated`   `#151820`   Menus, popovers, elevated surfaces
  `bg.control`    `#191C23`   Inputs and controls
  `bg.hover`      `#1D2027`   Hover states

### Borders

  Token              Value         Usage
  ------------------ ------------- ----------------------------
  `border.default`   `#252932`     Standard structural border
  `border.strong`    `#30343D`     Stronger separation
  `border.focus`     `#159EAD`     Focused controls
  `border.accent`    `#159EAD66`   Subtle active state

Default border width: **1px**.\
Use **2px** only for strong active indicators.

### Official accent: Petroleum Blue

  Token              Value         Usage
  ------------------ ------------- ----------------------------
  `accent.primary`   `#159EAD`     Primary accent
  `accent.hover`     `#1BAEBD`     Hover
  `accent.active`    `#128B99`     Pressed/active
  `accent.subtle`    `#159EAD1A`   Subtle selected background
  `accent.strong`    `#159EAD33`   Strong selected background
  `accent.border`    `#159EAD66`   Accent border

**Rule:** Petroleum Blue means interaction, not decoration.

Use it for selection, focus, active tools, primary actions, current
controls, links, and editing indicators. Do not flood the interface with
blue.

### Semantic colors

  Category          Color
  ----------------- -----------
  Video             `#3D5F91`
  Audio             `#3D8A62`
  Effects           `#8C523C`
  Adjustment        `#68508C`
  Text / Graphics   `#8B7A3E`
  Error             `#D95C5C`
  Warning           `#D5A84A`

Keep semantic colors restrained.

------------------------------------------------------------------------

## 4. Typography

### Primary UI font

**Inter**

Weights:

-   400 --- Regular
-   500 --- Medium
-   600 --- Semibold

Avoid 700 unless specifically necessary.

### Technical font

**IBM Plex Mono**

Use for:

-   Timecode
-   FPS
-   Resolution
-   Numeric technical values
-   Metadata
-   Shortcuts
-   Video parameters
-   Precise measurements

------------------------------------------------------------------------

## 5. Type Scale

  Token              Size   Line Height    Weight Usage
  ---------------- ------ ------------- --------- ------------------
  `text.xs`          10px          14px       400 Metadata
  `text.sm`          11px          14px   400/500 Labels
  `text.md`          12px          16px   400/500 Standard UI
  `text.lg`          13px          18px       500 Small headings
  `text.xl`          14px          20px   500/600 Section headings
  `text.2xl`         16px          22px       600 Major titles
  `text.display`     20px          24px       600 Home headings

Technical:

  Token         Size   Line Height
  ----------- ------ -------------
  `mono.sm`     10px          14px
  `mono.md`     11px          14px
  `mono.lg`     12px          16px

------------------------------------------------------------------------

## 6. Spacing System

oca uses a **4px base spacing unit**.

  Token          Value
  ------------ -------
  `space.1`        4px
  `space.2`        8px
  `space.3`       12px
  `space.4`       16px
  `space.5`       20px
  `space.6`       24px
  `space.8`       32px
  `space.10`      40px
  `space.12`      48px

### Common spacing

  Relationship            Value
  --------------------- -------
  Icon → Label              8px
  Label → Shortcut         16px
  Label → Control           6px
  Control → Control         8px
  Panel padding            12px
  Large panel padding      16px

Avoid arbitrary spacing values whenever possible.

------------------------------------------------------------------------

## 7. Border Radius

  Token             Value Usage
  --------------- ------- ----------------------------------
  `radius.none`       0px Workspace panels
  `radius.xs`         2px Tiny technical elements
  `radius.sm`         3px Buttons, inputs, menus, tooltips
  `radius.md`         4px Cards, dialogs

**Never use 8px+ radius for standard oca UI.**

------------------------------------------------------------------------

## 8. Elevation and Shadows

oca relies primarily on contrast and borders.

Standard surfaces should have no visible shadow.

Menus, popovers, and dialogs may use:

``` text
0 8px 24px rgba(0, 0, 0, 0.35)
```

Do not use decorative shadows.

------------------------------------------------------------------------

## 9. Buttons

### Small Button

``` text
Height: 24px
Padding X: 8px
Padding Y: 4px
Font: Inter 11px
Weight: 500
Radius: 3px
```

### Medium Button

``` text
Height: 28px
Padding X: 12px
Padding Y: 6px
Font: Inter 12px
Weight: 500
Radius: 3px
```

### Primary Button

``` text
Height: 32px
Padding X: 12px
Padding Y: 8px
Font: Inter 12px
Weight: 500
Radius: 3px
```

### Icon + label

``` text
[icon]  Label
   8px
```

Icon-to-label spacing is **8px**.

------------------------------------------------------------------------

## 10. Button States

Every button defines:

-   Default
-   Hover
-   Pressed
-   Focused
-   Disabled
-   Loading

### Primary

  State      Background
  ---------- ------------
  Default    `#159EAD`
  Hover      `#1BAEBD`
  Pressed    `#128B99`
  Disabled   `#26383C`

### Secondary

  State     Background
  --------- ------------
  Default   `#191C23`
  Hover     `#20242C`
  Pressed   `#151820`

Avoid glow effects.

------------------------------------------------------------------------

## 11. Icon Buttons

### Compact

``` text
24 × 24px
Icon: 16 × 16px
Padding: 4px
Radius: 3px
```

### Toolbar

``` text
28 × 28px
Icon: 16px
Radius: 3px
```

### Icon colors

  State      Color
  ---------- -----------
  Default    `#777A86`
  Hover      `#C5C7CF`
  Active     `#FFFFFF`
  Accent     `#159EAD`
  Disabled   `#41434E`

------------------------------------------------------------------------

## 12. Menu Bar

### Container

``` text
Height: 32px
Horizontal padding: 8px
Background: #090A0D
```

### Menu item

``` text
Height: 28px
Padding X: 8px
Padding Y: 6px
Font: Inter 12px / 16px
Weight: 500
Radius: 3px
```

Example:

``` text
File   Edit   View   Sequence   Clip   Effects   Markers
```

------------------------------------------------------------------------

## 13. Dropdown Menu

### Container

``` text
Background: #11141A
Border: 1px solid #30343D
Radius: 4px
Padding: 4px 0
Minimum width: 208px
```

Typical larger menu width:

``` text
240–280px
```

Do not let menu width vary arbitrarily based only on text.

------------------------------------------------------------------------

## 14. Menu Item

``` text
Height: 28px
Padding Left: 12px
Padding Right: 8px
```

Layout:

``` text
[icon]  Label                         Shortcut
         ← 8px →                     ← 16px →
```

### Typography

Label:

``` text
Inter
12px / 16px
Weight: 400
```

Shortcut:

``` text
IBM Plex Mono
10px / 14px
Weight: 400
Color: #626874
```

Example:

``` text
New Project...                    ⌘ N
Open Project...                   ⌘ O
Save                              ⌘ S
```

------------------------------------------------------------------------

## 15. Menu States

### Default

``` text
Background: transparent
Text: #969BA6
```

### Hover

``` text
Background: #1A2027
Text: #FFFFFF
```

### Active / Selected

``` text
Background: #159EAD1A
Text: #FFFFFF
Left indicator: 2px #159EAD
```

The Petroleum Blue indicator is intentionally subtle.

------------------------------------------------------------------------

## 16. Menu Separators

``` text
Height: 1px
Color: #252932
Margin: 4px 8px
```

Separators should not touch the menu edges.

------------------------------------------------------------------------

## 17. Submenus

Submenus use the same tokens as the main menu.

``` text
Export                         >
Open Recent                    >
```

Chevron:

``` text
12px
```

Right padding:

``` text
8px
```

Avoid animation-heavy submenu behavior.

------------------------------------------------------------------------

## 18. Disabled Menu Items

``` text
Text: #41454E
Shortcut: #343840
Background: transparent
```

Keep disabled commands visible when their existence communicates
available functionality.

------------------------------------------------------------------------

## 19. Tooltips

``` text
Background: #191C23
Border: 1px solid #30343D
Radius: 3px
Padding: 6px 8px
Font: Inter 11px / 14px
Delay: 500ms
```

Shortcuts use IBM Plex Mono.

------------------------------------------------------------------------

## 20. Inputs

``` text
Height: 28px
Padding: 0 8px
Border: 1px solid #30343D
Radius: 3px
Background: #151820
Font: Inter 12px / 16px
```

Focus:

``` text
Border: #159EAD
```

Avoid large web-style focus rings.

------------------------------------------------------------------------

## 21. Dropdown / Select

``` text
Height: 28px
Padding: 0 8px
Border: 1px solid #30343D
Radius: 3px
```

Arrow:

``` text
12px
```

Open state:

``` text
Border: #159EAD66
```

------------------------------------------------------------------------

## 22. Sliders

``` text
Track height: 2px
Thumb: 8 × 8px
Active: #159EAD
Inactive: #343943
```

Controls remain compact.

------------------------------------------------------------------------

## 23. Panels

### Standard

``` text
Background: #101216
Border: 1px solid #252932
Padding: 12px
Radius: 0px
```

### Large

``` text
Background: #101216
Border: 1px solid #252932
Padding: 16px
Radius: 0px
```

Workspace panels should visually read as parts of one workstation.

------------------------------------------------------------------------

## 24. Toolbar

``` text
Height: 28px
Icon: 16px
Horizontal gap: 4px
```

Toolbar groups may use:

``` text
Divider:
1px × 16px
Color: #252932
Margin: 8px
```

Avoid wrapping every group in rounded containers.

------------------------------------------------------------------------

## 25. Tool Rail

The Tool Rail is a signature oca component.

``` text
Tool button:
28 × 28px

Icon:
16px

Radius:
3px
```

Active:

``` text
Background: #159EAD1A
Icon: #FFFFFF
Left indicator: 2px #159EAD
```

Suggested tools:

``` text
Selection
Cut
Slip
Text
Transform
```

------------------------------------------------------------------------

## 26. Media Browser

Media items should expose technical information without becoming generic
cards.

Example:

``` text
Interview_Hero
02:34   3840 × 2160
        23.976 FPS
```

On hover, additional metadata may appear:

``` text
3840 × 2160
23.976 FPS
H.264
02:34
```

Avoid excessive badges.

------------------------------------------------------------------------

## 27. Thumbnail System

When enough horizontal space exists, use multiple video frames:

``` text
┌────┬────┬────┬────┐
│    │    │    │    │
│    │    │    │    │
└────┴────┴────┴────┘
```

This reinforces the frame-based CINE identity.

------------------------------------------------------------------------

## 28. Program Monitor

The Program Monitor should resemble professional video equipment rather
than a consumer media player.

Header:

``` text
PROGRAM MONITOR                         50%   FIT   100%
```

Viewer:

``` text
┌───────────────────────────────────────┐
│                                       │
│                                       │
│              VIDEO FRAME              │
│                                       │
│                                       │
│                                       │
└───────────────────────────────────────┘
```

Technical information may appear contextually.

------------------------------------------------------------------------

## 29. Timecode

Timecode is a key oca identity element.

``` text
00:02:14:08
```

Use:

``` text
IBM Plex Mono
12px
```

Optional label:

``` text
TC  00:02:14:08
```

------------------------------------------------------------------------

## 30. Playback Controls

Playback should communicate precision rather than consumer media
playback.

Recommended:

``` text
|◀     ▶     ▶|
```

Keep controls compact.

------------------------------------------------------------------------

## 31. Timeline

The Timeline is the **heart of oca**.

### Header

``` text
TIMELINE                              +    ZOOM ─────●──
SEQ 01 / MAIN
```

Recommended header height:

``` text
32px
```

------------------------------------------------------------------------

## 32. Timeline Ruler

The ruler must adapt its density to zoom.

``` text
00:00:00       00:00:30       00:01:00
│    │    │    │    │    │    │
```

Major ticks use stronger contrast.

Minor ticks remain subtle.

Time labels use IBM Plex Mono.

------------------------------------------------------------------------

## 33. Playhead

Recommended shape:

``` text
        ▼
        │
        │
        │
```

Recommended color:

``` text
#FF5B67
```

The playhead should remain visually distinct from Petroleum Blue
selection.

Do not use glow.

------------------------------------------------------------------------

## 34. Timeline Clips

Large clip:

``` text
┌────────────────────────────────────────┐
│ Interview_Hero                         │
│ ────────────────────────────────────── │
│ ▪ ▪ ▪ ▪ ▪ ▪ ▪ ▪ ▪ ▪ ▪ ▪ ▪             │
└────────────────────────────────────────┘
```

Small clip:

``` text
┌─────────────────────┐
│ Interview_Hero      │
└─────────────────────┘
```

Secondary information should disappear progressively as clip width
decreases.

------------------------------------------------------------------------

## 35. Clip Selection

Do not simply turn selected clips blue.

Use:

-   Subtle accent background
-   Strong outline
-   Trim handles
-   Contextual information

Recommended:

``` text
Border: 1px #159EAD
Background: existing clip color + subtle accent
```

------------------------------------------------------------------------

## 36. Audio Clips

Audio clips should contain:

-   Waveform
-   Baseline
-   Clip name when space allows
-   Volume information where appropriate

Waveforms should be organic and readable.

------------------------------------------------------------------------

## 37. Track Headers

Example:

``` text
VIDEO 1
  ◉  🔒  🔊
```

The track name has priority.

Recommended:

``` text
Width: 120–160px
Padding: 8px
```

------------------------------------------------------------------------

## 38. Track Category Indicators

Use a small colored line rather than coloring the entire track.

``` text
│ VIDEO 1
│ VIDEO 2
│ AUDIO 1
│ AUDIO 2
│ FX
```

------------------------------------------------------------------------

## 39. Effects Browser

Effects should be organized like a professional library, not generic
cards.

``` text
EFFECTS

⌕ Search effects...

ALL    VIDEO    COLOR    AUDIO

RECENT
────────────────────

Color
  Exposure
  Contrast
  Curves
  Color Balance

Stylize
  Film Grain
  Vignette
  Glow

Blur
  Gaussian
  Radial
```

------------------------------------------------------------------------

## 40. Inspector

The right-side panel is contextual.

Possible contexts:

-   Project
-   Clip
-   Effect
-   Text
-   Audio
-   Transform
-   Color

Example:

``` text
INSPECTOR

TRANSFORM

Position

X              960.0
Y              540.0

Scale           100%

Rotation          0°

Anchor

X              960.0
Y              540.0
```

------------------------------------------------------------------------

## 41. Contextual UI

  Selection          Inspector
  ------------------ -----------
  Nothing            Project
  Video clip         Clip
  Audio clip         Audio
  Effect             Effect
  Text               Text
  Transform          Transform
  Color adjustment   Color

oca should create intelligence through context rather than AI
decoration.

------------------------------------------------------------------------

## 42. Keyframes

Use a consistent visual language:

``` text
◇  inactive
◆  active
```

Use the same language across:

-   Inspector
-   Timeline
-   Effects
-   Animation
-   Audio automation

------------------------------------------------------------------------

## 43. Home / Project Browser

Cards are appropriate for projects because projects are distinct
entities.

Project card:

``` text
Border: 1px #252932
Radius: 4px
```

Thumbnail:

``` text
Radius: 3px 3px 0 0
```

Content:

``` text
Padding: 8px
```

Metadata:

``` text
Inter 10px
Color: #626874
```

Avoid excessive cards, shadows, gradients, and oversized rounded
containers.

------------------------------------------------------------------------

## 44. Status Bar

Example:

``` text
Project: Documentary
Sequence: Main
24 FPS
1920 × 1080
GPU
Ready
```

Technical values use IBM Plex Mono where appropriate.

------------------------------------------------------------------------

## 45. Notifications

Avoid large SaaS-style toast notifications for normal rendering status.

Prefer contextual status:

``` text
● Preview rendering...
```

or:

``` text
PREVIEW
Rendering frame 1248...
```

------------------------------------------------------------------------

## 46. Loading States

Prefer informative contextual states:

``` text
PREVIEW
Rendering frame 1248...
```

or:

``` text
● Rendering preview...
```

Loading states should communicate what the application is doing.

------------------------------------------------------------------------

## 47. Empty States

Avoid marketing-style copy.

Use:

``` text
NO MEDIA

Import media to begin.

⌘ I
```

------------------------------------------------------------------------

## 48. Error States

Use actionable technical information.

``` text
MEDIA ERROR

Unable to decode frame.

H.264 / 10-bit
Frame 1824

Retry
```

Avoid vague messages such as "Oops! Something went wrong."

------------------------------------------------------------------------

## 49. Motion

oca prioritizes speed over spectacle.

  Interaction     Duration
  ------------- ----------
  Hover               80ms
  Focus              100ms
  Panel              120ms
  Menu               100ms
  Dialog             150ms

Avoid slow 500--800ms animations and spring-heavy motion for ordinary
controls.

------------------------------------------------------------------------

## 50. Interaction States

Every interactive component must define:

``` text
Default
Hover
Pressed
Focused
Selected
Disabled
Loading
Error
```

------------------------------------------------------------------------

## 51. Design Token Architecture

Use semantic tokens instead of hardcoded values.

``` text
cine.bg.canvas
cine.bg.panel
cine.bg.elevated
cine.bg.control
cine.bg.hover

cine.border.default
cine.border.strong
cine.border.focus
cine.border.accent

cine.text.primary
cine.text.secondary
cine.text.tertiary
cine.text.disabled

cine.accent.primary
cine.accent.hover
cine.accent.active
cine.accent.subtle
cine.accent.strong

cine.semantic.video
cine.semantic.audio
cine.semantic.effect
cine.semantic.adjustment
cine.semantic.text
cine.semantic.warning
cine.semantic.error

cine.radius.none
cine.radius.xs
cine.radius.sm
cine.radius.md

cine.space.1
cine.space.2
cine.space.3
cine.space.4
cine.space.5
cine.space.6
cine.space.8
cine.space.10
cine.space.12
```

------------------------------------------------------------------------

## 52. Component Architecture

``` text
oca UI
│
├── Foundation
│   ├── Colors
│   ├── Typography
│   ├── Spacing
│   ├── Icons
│   ├── Borders
│   └── Motion
│
├── Navigation
│   ├── AppBar
│   ├── MenuBar
│   ├── Menu
│   └── ToolRail
│
├── Media
│   ├── MediaBrowser
│   ├── MediaItem
│   ├── Thumbnail
│   └── Metadata
│
├── Viewer
│   ├── ProgramMonitor
│   ├── Timecode
│   ├── PlaybackControls
│   └── ViewerOverlay
│
├── Timeline
│   ├── TimelineHeader
│   ├── TimeRuler
│   ├── Track
│   ├── TrackHeader
│   ├── VideoClip
│   ├── AudioClip
│   ├── EffectClip
│   ├── Playhead
│   ├── Marker
│   └── Keyframe
│
├── Inspector
│   ├── Section
│   ├── NumericInput
│   ├── Slider
│   ├── Toggle
│   └── KeyframeControl
│
└── Effects
    ├── EffectBrowser
    ├── EffectItem
    └── EffectParameters
```

------------------------------------------------------------------------

## 53. Figma Implementation Rules

The Figma design should use variables/tokens for:

-   Colors
-   Typography
-   Spacing
-   Radius
-   Borders
-   Component heights
-   Component padding

Every component should use Auto Layout.

Every component should expose variants for:

-   Default
-   Hover
-   Pressed
-   Focused
-   Selected
-   Disabled

Menus, buttons, inputs, toolbars, timeline clips, and inspector controls
should be reusable components.

------------------------------------------------------------------------

## 54. egui Implementation Rules

The same design system should map cleanly to the Rust/egui
implementation.

Do not scatter raw visual constants throughout the code.

Prefer a centralized theme:

``` rust
struct ocaTheme {
    bg_canvas: Color32,
    bg_panel: Color32,
    bg_elevated: Color32,

    border_default: Color32,
    border_strong: Color32,

    text_primary: Color32,
    text_secondary: Color32,
    text_tertiary: Color32,

    accent_primary: Color32,
    accent_hover: Color32,
    accent_active: Color32,

    spacing: SpacingTokens,
    radius: RadiusTokens,
}
```

The exact implementation can evolve, but visual constants should have a
single source of truth.

------------------------------------------------------------------------

## 55. Density

oca intentionally favors **high information density**.

The solution is not large spacing.

Instead use:

-   Strong hierarchy
-   Small typography
-   Compact controls
-   Clear alignment
-   Consistent spacing
-   Contextual information

The user should be able to see a lot without the interface becoming
noisy.

------------------------------------------------------------------------

## 56. Professionalism Test

The final interface should feel closer to:

> **A specialized professional instrument**

than:

> **A polished web application**

The visual hierarchy should communicate:

``` text
Precision
    ↓
Information
    ↓
Context
    ↓
Action
```

not:

``` text
Decoration
    ↓
Cards
    ↓
Gradient
    ↓
Marketing
```

------------------------------------------------------------------------

## 57. Final Visual Direction

``` text
DARK WORKSPACE
        +
PETROLEUM BLUE
        +
INTER
        +
IBM PLEX MONO
        +
4PX SPACING
        +
1PX STRUCTURAL BORDERS
        +
3PX CONTROL RADIUS
        +
COMPACT CONTROLS
        +
HIGH INFORMATION DENSITY
        +
CONTEXTUAL UI
        +
CINEMATIC DETAILS
```

The interface remains predominantly monochrome.

Petroleum Blue appears primarily when the user is **interacting with the
system**.

------------------------------------------------------------------------

# 58. North Star

> **oca should look inevitable.**
>
> Not futuristic for the sake of being futuristic.
>
> Not "AI-powered" for the sake of visual trends.
>
> Not like a generic SaaS application.
>
> It should look like a professional video editor designed by people who
> deeply understand editing.
>
> **Familiar structure. Distinctive execution.**

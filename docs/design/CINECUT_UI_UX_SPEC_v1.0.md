# oca — UI/UX Specification
Version: 1.0
Status: Implementation-ready baseline
Scope: Desktop professional video editor
Primary platform: Desktop
Design direction: Familiar structure. Distinctive execution.
Visual concept: FRAME × CUT

---

## 1. Product UI Principles

oca is a professional video editor. The interface must prioritize editing speed, precision, information density, discoverability, and predictable behavior.

### Non-negotiable principles

1. The workspace is the product.
2. Familiar structure, distinctive execution.
3. Dense but never cramped.
4. Functional color, not decorative color.
5. Technical information uses a technical visual language.
6. Selection and focus must always be obvious.
7. Every destructive action requires clear confirmation or an undo path.
8. Contextual UI beats permanent UI.
9. Do not hide core editing actions behind unnecessary menus.
10. Avoid visual noise.

### Explicitly avoid

- Glassmorphism
- Neon/glow effects
- Gradients
- Giant typography
- Excessive rounded corners
- Excessive shadows
- Excessive cards
- Pill-shaped controls
- Purple as an accent
- Decorative iconography
- Generic SaaS/dashboard aesthetics
- "AI" visual language
- Long spring animations

---

# 2. Design Tokens

## 2.1 Colors

### Surfaces

| Token | Value | Usage |
|---|---|---|
| bg-canvas | #090A0D | Application background |
| bg-panel | #101216 | Major panels |
| bg-elevated | #151820 | Elevated controls |
| bg-control | #191C23 | Hover/control surfaces |

### Borders

| Token | Value | Usage |
|---|---|---|
| border-default | #252932 | Normal separation |
| border-strong | #343945 | Strong separation |
| border-accent | #159EAD66 | Focus/open/active |
| border-active | #159EAD | Strong active indicator |

### Text

| Token | Value | Usage |
|---|---|---|
| text-primary | #E5E7EB | Main text |
| text-secondary | #969BA6 | Secondary labels |
| text-tertiary | #626874 | Metadata |
| text-disabled | #41454E | Disabled |

### Accent

Petroleum Blue is the official oca accent.

| Token | Value |
|---|---|
| accent-primary | #159EAD |
| accent-hover | #1BAEBD |
| accent-active | #128B99 |
| accent-subtle | #159EAD1A |
| accent-strong | #159EAD33 |

Petroleum Blue communicates:
- selection
- focus
- active state
- primary action
- playhead-related emphasis when appropriate
- keyframe active state
- current tool

It must NOT be used as decoration.

### Semantic timeline colors

Semantic colors identify content types and must remain visually subordinate to the interface accent.

- Video: restrained blue/steel
- Audio: restrained green
- FX: restrained amber
- Text/graphics: restrained neutral/semantic variant

Semantic colors identify object type. Petroleum Blue identifies interaction state.

---

# 3. Typography

UI font: Inter.

Technical font: IBM Plex Mono.

### UI scale

- 10/14 — technical metadata only
- 11/14 — compact metadata
- 12/16 — normal compact UI
- 13/18 — important UI
- 14/20 — primary control text
- 16/22 — section emphasis
- 20/24 — major workspace headings

### Rules

- Top-level menus: Inter 12/16, 500
- Dropdown labels: Inter 12/16, 400
- Shortcuts: IBM Plex Mono 10/14
- Timecode: IBM Plex Mono 10–12
- Numeric inspector values: IBM Plex Mono 10–12
- Do not use 10px for important operational information.

---

# 4. Spacing

Base unit: 4px.

Allowed scale:
4, 8, 12, 16, 20, 24, 32, 40, 48.

Rules:
- icon → label: 8px
- label → shortcut: 16px
- label → control: 6px
- control → control: 8px
- panel padding: 12px
- large panel padding: 16px
- section → section: 24px

---

# 5. Radius and Borders

Radius:
- workspace/panels: 0px
- technical micro elements: 2px
- controls/buttons/menus/tooltips: 3px
- dialogs/cards: 4px
- never default to 8px+

Borders:
- normal: 1px #252932
- strong: 1px #343945
- active: 1px #159EAD66
- strong active indicator: 2px #159EAD

---

# 6. Global Interaction Model

Every interactive element supports these conceptual states:

1. Default
2. Hover
3. Pressed
4. Focused
5. Disabled
6. Loading, when applicable

### Hover

Duration: ~80ms.

Hover changes:
- surface tint
- text brightness
- border when useful

Do not use scale transforms.

### Focus

Duration: ~100ms.

Keyboard focus must be visible using:
- petroleum-blue border or
- petroleum-blue focus ring

### Panel transitions

~120ms.

### Menus

~100ms.

### Modal/dialog

~150ms.

Animations must never obscure state changes.

---

# 7. Application Shell

The main editor has five permanent regions:

1. Application/Menu Bar
2. Left Tool/Media region
3. Program Monitor
4. Contextual Inspector region
5. Timeline

Optional panels may replace or split the left/right regions.

---

# 8. Top Application Bar

Height: 32px.

Left to right:

1. oca logo
2. Project selector
3. Sequence selector
4. File
5. Edit
6. View
7. Sequence
8. Clip
9. Markers
10. Graphics
11. Window
12. Help

Right side:

13. Timecode
14. FPS
15. Resolution
16. Export
17. Settings

---

## 8.1 oca logo

### Click

Opens application menu.

Menu:
- About oca
- Preferences
- Check for Updates
- Keyboard Shortcuts
- Documentation
- Quit

### Right click

Same application menu.

---

## 8.2 Project selector

Displays:
`Project: Documentary`

### Click

Opens project menu:

- Project Settings
- Project Media
- Project Info
- Save Project
- Save Project As...
- Close Project

### Project Settings opens

Dialog with:
- General
- Video
- Audio
- Cache
- Proxy
- Scratch/Storage

---

## 8.3 Sequence selector

Displays:
`Sequence: Interview`

### Click

Dropdown:
- current sequence
- New Sequence...
- Duplicate Sequence...
- Rename...
- Delete Sequence...
- Sequence Settings...

Selecting another sequence loads it into the timeline and program monitor.

---

# 9. File Menu

### New

Submenu:
- Project
- Sequence
- Bin
- Title/Graphic

### Open

Opens native file picker.

Supported project/document types are shown according to implementation.

### Save

Saves current project.

### Save As

Opens save dialog and creates a new project file.

### Import

Opens file picker.

### Export

Opens Export workflow.

### Project Settings

Opens Project Settings dialog.

### Preferences

Opens Preferences.

### Quit

Closes oca.

If unsaved changes exist:
- Save
- Don't Save
- Cancel

---

# 10. Edit Menu

### Undo

Undo last reversible action.

Shortcut: Ctrl/Cmd + Z.

### Redo

Redo last undone action.

Shortcut: Ctrl/Cmd + Shift + Z.

### Cut

Cuts selected timeline/media item.

### Copy

Copies selection.

### Paste

Pastes into active context.

### Paste Attributes

Opens attribute-selection dialog when applicable.

### Duplicate

Duplicates selected object.

### Delete

Deletes selected object.

### Select All

Selects all items in current context.

### Deselect All

Clears selection.

### Find

Opens contextual search.

---

# 11. View Menu

### Workspace

Submenu:
- Editing
- Color
- Audio
- Effects
- Graphics
- Custom

Selecting a workspace rearranges visible panels.

### Show/Hide Panels

Submenu:
- Media
- Inspector
- Effects
- Audio
- Timeline
- Program Monitor

### Program Monitor Overlays

Toggles:
- Safe Areas
- Grid
- Guides
- Metadata
- Camera/Recording overlay

### Zoom

Submenu:
- 25%
- 50%
- 75%
- 100%
- 200%
- Fit
- Fill
- Actual Size

### Full Screen

Toggles application full screen.

### Reset Workspace

Restores the active workspace layout.

---

# 12. Sequence Menu

### New Sequence

Opens sequence creation dialog.

Fields:
- Name
- Resolution
- Frame rate
- Audio sample rate
- Timebase

### Sequence Settings

Opens current sequence configuration.

### Render

Renders timeline preview.

### Render Selection

Renders only selected timeline range.

### Add Track

Submenu:
- Video Track
- Audio Track
- FX Track

### Delete Track

Deletes selected track after confirmation if it contains content.

---

# 13. Clip Menu

### Enable/Disable

Toggles clip participation in playback/render.

### Link/Unlink

Links video and audio components.

### Group/Ungroup

Groups timeline objects.

### Nest

Creates a nested sequence.

### Speed/Duration

Opens Speed/Duration dialog.

### Frame Hold

Submenu:
- Hold on current frame
- Hold on In
- Hold on Out

### Audio Options

Submenu:
- Audio Gain
- Channels
- Normalize

---

# 14. Markers Menu

### Add Marker

Creates marker at playhead.

### Add Marker at In/Out

Creates marker at range boundary.

### Edit Marker

Opens marker editor.

### Delete Marker

Deletes selected marker.

### Delete All Markers

Deletes all markers after confirmation.

Marker data:
- name
- color
- comment
- duration/range

---

# 15. Graphics Menu

### New Text

Creates a text layer at playhead.

### New Shape

Creates shape graphic.

### Open Graphics Panel

Opens graphics editor.

### Align

Submenu:
- Left
- Center
- Right
- Top
- Middle
- Bottom

### Distribute

Submenu:
- Horizontal
- Vertical

---

# 16. Window Menu

Controls panel visibility and workspace arrangement.

Items:
- Media
- Inspector
- Effects
- Audio
- Program Monitor
- Timeline
- Audio Meters
- Metadata
- Workspace presets

---

# 17. Help Menu

- Keyboard Shortcuts
- Documentation
- Tutorials
- Report a Problem
- Check for Updates
- About oca

---

# 18. Export Button

Primary action.

Height: 32px.

### Click

Opens Export workspace/dialog.

Export UI:

Left:
- Format
- Preset
- Video
- Audio
- Captions
- Metadata

Center:
- preview/output information

Right:
- estimated output
- destination
- filename
- Export button

### Export button inside dialog

Starts export.

During export:
- progress indicator
- elapsed time
- estimated remaining time
- Cancel

After completion:
- Open File
- Reveal in Folder
- Close

---

# 19. Settings Button

Gear icon.

### Click

Opens Preferences.

Sections:
- General
- Appearance
- Editing
- Timeline
- Playback
- Audio
- Cache
- Proxy
- Keyboard
- Shortcuts
- Performance

---

# 20. Left Tool Rail

Vertical toolbar.

Tools:

1. Selection
2. Razor/Cut
3. Trim
4. Slip/Slide
5. Text
6. Effects
7. Hand/Pan
8. Zoom

Each tool has:
- icon
- tooltip
- active state
- shortcut

### Selection Tool

Default.

Used to:
- select clips
- move clips
- resize clips
- select keyframes
- interact with panels

### Razor Tool

Click a clip to split it at playhead/cursor position.

Click-drag can define a cut range where supported.

### Trim Tool

Allows edge trimming and trim-mode editing.

### Slip/Slide Tool

Changes clip content or clip position without changing overall sequence duration where applicable.

### Text Tool

Click program monitor to create text.

### Effects Tool

Activates effect placement/interaction.

### Hand Tool

Pans timeline or monitor canvas.

### Zoom Tool

Zooms visual workspace/timeline depending on active region.

---

# 21. Media Panel

Tabs:

- Media
- Bins
- Favorites
- Recent

## Search field

### Type

Filters current media view.

Search matches:
- filename
- type
- metadata
- tags

### Clear

Returns to unfiltered media.

## Filter button

Opens filter menu:
- Video
- Audio
- Image
- Graphics
- Favorites
- Used
- Unused

## Grid/List toggle

Switches between thumbnail grid and compact list.

---

# 22. Project Media

Header:
`Project Media`

Displays item count.

### Expand/collapse arrow

Expands/collapses the media section.

### Add button

Opens menu:
- Import Media
- New Bin
- New Smart Bin

### Media card click

Single click:
- selects asset
- updates metadata/details

Double click:
- opens asset in source/preview viewer

Drag:
- drags media into timeline

Right click

Context menu:
- Open
- Rename
- Reveal in Project
- Reveal on Disk
- Replace Media
- Make Offline
- Properties
- Add to Bin
- Add to Favorites
- Delete from Project

---

# 23. Media Thumbnail Controls

Three-dot menu:

- Open
- Rename
- Properties
- Replace
- Reveal
- Delete

Thumbnail selection:
- border becomes petroleum blue
- no glow

---

# 24. Audio Media Items

Audio assets use a compact audio representation.

Click:
- select

Double click:
- preview audio

Drag:
- insert into active audio track.

---

# 25. Program Monitor

Purpose: preview current sequence output.

Top bar:
- PROGRAM MONITOR
- Zoom
- Fit
- 100%
- More

---

## 25.1 Zoom selector

### Click

Dropdown:

- 25%
- 50%
- 75%
- 100%
- 200%
- Fit
- Fill
- Actual Size

### Meaning

Fit:
entire frame visible.

Fill:
frame fills monitor while maintaining aspect ratio.

Actual Size:
1:1 pixel display.

---

# 26. Program Monitor More Menu

Contains:

- Safe Areas
- Grid
- Guides
- Show Metadata
- Show Camera Overlay
- Show Timecode
- Show Frame Number
- Transparent Grid, when applicable

Each item is a toggle.

---

# 27. Program Monitor Overlays

### CAM 01

Camera/source label.

Not interactive by default.

### REC indicator

Visual recording/status indicator.

Not an action unless recording functionality is implemented.

### Resolution/FPS overlay

Technical metadata.

### TC overlay

Current timecode and frame information.

---

# 28. Program Monitor Playback Controls

Controls:

1. Go to In
2. Previous Frame/Previous Edit
3. Play/Pause
4. Next Frame/Next Edit
5. Go to Out
6. Loop
7. Snapshot
8. Viewer/Audio options

### Play/Pause

Starts or stops sequence playback.

Spacebar mirrors this action.

### Loop

Toggles playback looping between In/Out or sequence bounds.

### Snapshot

Captures current program frame.

Opens:
- format choice
- destination
- save

---

# 29. Inspector

The Inspector is contextual.

Tabs:
- Inspector
- Effects
- Audio

The content changes according to selection.

---

# 30. Inspector — No Selection

Show:

`Nothing selected`

Secondary guidance:
`Select a clip, track, graphic, or effect to edit its properties.`

Do not display meaningless controls.

---

# 31. Inspector — Video Clip

Header:
- filename
- media type
- resolution
- frame rate
- duration
- overflow menu

Overflow menu:
- Rename
- Reveal Media
- Replace Media
- Properties
- Disable
- Delete

Sections:

1. Transform
2. Crop
3. Composite
4. Speed

---

# 32. Transform

Fields:

### Position X/Y

Numeric fields.

Click:
- focus numeric input.

Drag:
- scrub numeric value horizontally.

Double click:
- reset/edit value.

### Scale

Slider + numeric value.

Drag slider:
changes scale.

Click numeric:
allows exact input.

### Rotation

Slider + numeric.

### Anchor

X/Y numeric values.

### Reset icon

Resets entire Transform section to defaults.

### Keyframe diamond

Click:
- adds keyframe at current playhead.

If keyframe exists:
- clicking toggles/removes current keyframe depending on interaction convention.

Right click:
- keyframe options.

---

# 33. Crop

Fields:

- Left
- Right
- Top
- Bottom

Each supports:
- numeric entry
- horizontal scrubbing
- reset
- keyframe

Reset icon:
resets all crop values.

---

# 34. Composite

### Blend Mode

Dropdown.

Opens:
- Normal
- Dissolve
- Darken
- Multiply
- Screen
- Overlay
- Soft Light
- Hard Light
- Difference
- additional supported modes

### Opacity

Slider + numeric value.

---

# 35. Speed

Numeric percentage.

### Click

Allows exact speed.

### Right click / context

- Reset to 100%
- Reverse
- Speed/Duration...

---

# 36. Add Effect Button

### Click

Opens Effects browser with search focused.

Selecting an effect:
- adds it to the selected clip
- switches Inspector to the effect section.

If no clip is selected:
- button disabled.

---

# 37. Effects Panel

Organized by category, never as a generic icon list.

Categories:

- Blur & Sharpen
- Color
- Distortion
- Keying
- Light
- Stylize
- Transform
- Utility
- Audio, when applicable

### Search

Filters effects.

### Effect row click

Selects effect.

### Double click

Applies effect to selected clip.

### Drag effect

Drag onto clip to apply.

### Favorites

Marks frequently used effects.

---

# 38. Audio Panel

Tabs/sections:

- Clip
- Track
- Mixer, where available

Controls:
- Volume
- Pan
- Gain
- Mute
- Solo
- Channel configuration

Meters use clear level semantics.

---

# 39. Audio Meters

Right side vertical meters.

Shows:
- L
- R
- dB scale
- peak level

Green:
normal signal.

Yellow:
approaching high level.

Red:
clipping/overload.

Meters are informational and must not use Petroleum Blue as their signal color.

---

# 40. Timeline

The timeline is the primary editing surface.

Contains:

1. Timeline tab bar
2. Sequence ruler
3. Track headers
4. Video tracks
5. Audio tracks
6. FX tracks
7. Playhead
8. Clips
9. Transitions
10. Keyframes
11. Waveforms

---

# 41. Timeline Tab Bar

Current sequence:
`Interview`

### Close X

Closes sequence tab.

If unsaved:
show save/discard/cancel confirmation.

### Plus

Opens:
- New Sequence
- Open Sequence

---

# 42. Timeline Ruler

Shows timecode.

### Click

Moves playhead.

### Drag

Scrubs timeline.

### Double click

Context-dependent marker/time navigation if supported.

### Zoom

Mouse wheel + modifier or dedicated timeline zoom control.

---

# 43. Track Headers

Each track has:

- track name/number
- lock
- visibility/monitor
- mute where applicable
- solo where applicable
- expand/collapse
- additional track menu

---

# 44. Track Expand/Collapse Buttons

The `<` and `>` controls in the track header.

### Click

Toggles track height.

Collapsed:
- compact track row
- clips remain minimally represented
- waveforms/keyframes hidden when appropriate

Expanded:
- full clip representation
- waveforms
- keyframes
- effect controls

This is a per-track state.

### Keyboard

Optional shortcut can expand/collapse focused track.

---

# 45. Track Lock

### Click

Locks/unlocks track editing.

Locked:
- cannot move clips
- cannot trim
- cannot delete
- cannot modify track content

Playback remains active.

Visual:
lock icon becomes active.

---

# 46. Track Visibility

For video tracks:

### Click

Toggles track visibility.

Hidden:
- track is excluded from program output.

Does not delete content.

---

# 47. Track Mute

For audio/FX-capable tracks.

### Click

Toggles audio output.

Muted:
- no audio from track
- mute state clearly visible

Shortcut:
M, when track is focused, where no conflict exists.

---

# 48. Track Solo

If implemented:

### Click

Solo track.

Other audio tracks are temporarily excluded from playback.

Multiple solo tracks can coexist.

---

# 49. Track More Menu

Contains:

- Rename Track
- Duplicate Track
- Delete Track
- Add Track Above
- Add Track Below
- Move Track Up
- Move Track Down
- Track Color
- Track Height
- Select Track Content

Deleting a track containing clips requires confirmation.

---

# 50. Video Clip Interaction

### Single click

Select clip.

### Shift + click

Add/remove from selection.

### Drag

Move clip.

### Drag left/right edge

Trim.

### Alt/Option + drag

Duplicate clip.

### Right click

Context menu.

Context menu:
- Cut
- Copy
- Paste
- Duplicate
- Delete
- Enable/Disable
- Link/Unlink
- Group/Ungroup
- Speed/Duration
- Nest
- Audio Options
- Reveal Media
- Properties

---

# 51. Clip Selection

Selected clip:
- 1px petroleum-blue border
- subtle accent background where appropriate

Do not add glow.

Multiple selection:
- shared selection treatment
- individual clips remain visually distinguishable.

---

# 52. Clip Trimming

Dragging clip edge enters trim interaction.

Cursor changes.

While trimming, show:
- trim duration
- source/sequence timecode
- frame preview where supported

Release commits trim.

Undo reverses trim.

---

# 53. Transitions

Transitions appear between clips.

### Click

Selects transition.

Inspector changes to transition properties.

### Double click

Opens transition configuration.

Properties may include:
- duration
- alignment
- type
- direction
- parameters

### Delete

Removes transition and preserves adjacent clips.

---

# 54. Keyframes

Diamond indicators.

### Click empty keyframe control

Creates keyframe at playhead.

### Click existing keyframe

Selects keyframe.

### Drag keyframe

Moves time position.

### Right click

Menu:
- Delete
- Hold
- Linear
- Ease In
- Ease Out
- Bezier, where supported

Selected keyframe uses Petroleum Blue.

---

# 55. Playhead

The playhead indicates current editing position.

### Click ruler

Moves playhead.

### Drag playhead

Scrubs.

### Keyboard arrows

Move frame-by-frame.

### Shift + arrow

Move by larger increments where supported.

Playhead must remain visually dominant.

---

# 56. Snapping

Snapping is a timeline-level toggle.

### Click magnet icon

Toggles snapping.

When enabled, objects snap to:
- clip edges
- markers
- playhead
- sequence boundaries
- keyframes

When disabled:
free positioning.

---

# 57. Timeline Zoom

Zoom controls change timeline horizontal scale only.

Must not alter playback speed.

Zoom levels can be continuous.

The user must always be able to return to a useful overview.

---

# 58. Timeline Context Menu

Right click empty timeline area:

- Paste
- Add Marker
- Add Track
- Delete Empty Tracks
- Select All
- Deselect All
- Snapping
- Timeline Settings

---

# 59. Keyboard Shortcuts

Core shortcuts:

| Action | Shortcut |
|---|---|
| Undo | Ctrl/Cmd + Z |
| Redo | Ctrl/Cmd + Shift + Z |
| Save | Ctrl/Cmd + S |
| Copy | Ctrl/Cmd + C |
| Cut | Ctrl/Cmd + X |
| Paste | Ctrl/Cmd + V |
| Delete | Delete/Backspace |
| Play/Pause | Space |
| Select Tool | V |
| Razor | C |
| Text | T |
| Zoom | Z |
| Add Marker | M |
| Frame step | Left/Right |
| Jump In | I |
| Jump Out | O |

Shortcuts must be customizable.

---

# 60. Dialog System

Dialogs use:
- bg-elevated
- 4px radius
- strong border
- restrained shadow

Buttons:
- Cancel
- primary action

Primary action uses Petroleum Blue.

Destructive action should use semantic red only when necessary.

---

# 61. Confirmation Dialogs

Required for:

- deleting a project
- deleting a sequence with important unsaved content
- deleting populated tracks
- permanently removing media
- destructive cache/storage actions

Not required for ordinary clip deletion because Undo exists.

---

# 62. Toasts / Notifications

Avoid generic SaaS toast spam.

Use contextual status messages.

Examples:
- `Export complete`
- `3 clips copied`
- `Media relinked`
- `Render failed`

Each message:
- concise
- actionable when needed
- automatically disappears if informational
- persistent when user intervention is required

---

# 63. Loading States

Never freeze the interface without feedback.

Use contextual indicators:
- `Importing...`
- `Rendering...`
- `Generating waveform...`
- `Building proxy...`

Where possible, show progress.

---

# 64. Error States

Errors must explain:

1. What happened
2. What the user can do
3. Whether work was preserved

Example:

`Export failed`
`The selected codec could not be initialized.`

Actions:
- Change Settings
- Retry
- Close

---

# 65. Empty States

Empty states should be functional, not decorative.

Example Media:
`No media imported`
`Import video, audio, or images to start editing.`

Primary action:
`Import Media`

Example Inspector:
`Select a clip to edit its properties.`

---

# 66. Accessibility

Minimum target:

- keyboard navigation for all core controls
- visible focus state
- labels/tooltips for icon-only buttons
- sufficient contrast
- no information conveyed only by color
- target sizes generally >= 24px
- 28px controls for normal fields
- keyboard-accessible menus
- predictable tab order
- screen-reader names for controls where platform accessibility APIs are available

Important: screenshot review alone cannot establish WCAG compliance. Keyboard, contrast, scaling, focus order, and assistive technology require implementation testing.

---

# 67. Tooltips

Tooltip delay: ~500ms.

Use tooltips for:
- icon-only controls
- unfamiliar controls
- technical actions

Tooltip content:
`Name — Shortcut`

Example:
`Razor Tool — C`

Do not use tooltips for obvious text buttons.

---

# 68. Contextual Right Panel Rules

The right panel must react to selection.

### No selection
Neutral guidance.

### Video clip
Inspector → Transform/Crop/Composite/Speed.

### Audio clip
Inspector/Audio → Volume/Pan/Gain.

### Effect
Inspector → effect-specific parameters.

### Text
Inspector → typography/layout/appearance.

### Transition
Inspector → duration/type/parameters.

### Track
Inspector → track properties.

This prevents a static "property wall."

---

# 69. Responsive / Window Behavior

oca is desktop-first.

At smaller window sizes:
1. preserve timeline
2. preserve program monitor
3. collapse secondary panels
4. allow panel resizing
5. never arbitrarily shrink critical controls below usability

Panels should be dockable/resizable.

---

# 70. Panel Resizing

Panel separators have a resize cursor.

Dragging:
- changes panel width/height
- updates layout live

Double click separator:
- reset to default size, where supported.

Minimum widths:
- Media: enough for one useful thumbnail
- Inspector: enough for numeric controls
- Program Monitor: enough to display a usable frame
- Timeline: always retains minimum usable width

---

# 71. Context Menus

All context menus follow one visual system.

- bg: #11141A
- border: #30343D
- radius: 4px
- vertical padding: 4px
- item height: 28px
- left padding: 12px
- label: Inter 12/16
- shortcut: IBM Plex Mono 10/14
- hover: #1A2027
- selected: #159EAD1A
- active indicator: 2px petroleum-blue bar

Separators:
- 1px #252932
- horizontal margin 8px

---

# 72. Menu Behavior

Top-level menu item:
- 28px high
- 8px horizontal padding
- 3px radius

Hover:
- #191C23

Open:
- #1B2028
- subtle accent border

Dropdown:
- opens below item
- remains open while pointer moves through submenu path
- closes on outside click
- closes on Escape

Keyboard:
- Arrow keys navigate
- Enter activates
- Escape closes
- first-letter navigation where appropriate

---

# 73. State Hierarchy

oca uses the following visual priority:

1. Error / destructive
2. Current selection
3. Current focus
4. Active tool
5. Primary action
6. Hover
7. Passive information

Petroleum Blue should not appear simultaneously on dozens of unrelated elements.

---

# 74. Visual Identity Rules

The interface should look mostly neutral at first glance.

Petroleum Blue should become noticeable only after interaction.

This creates the oca signature:

- neutral workspace
- technical typography
- restrained semantic colors
- petroleum-blue interaction states
- dense timeline
- precise controls

---

# 75. Implementation Acceptance Criteria

A UI component is considered complete only if:

- default state exists
- hover state exists
- pressed state exists
- focus state exists
- disabled state exists where applicable
- keyboard behavior is defined
- tooltip exists for icon-only controls
- click behavior is defined
- resulting panel/menu/dialog is defined
- destructive behavior is defined
- undo behavior is defined where applicable
- spacing follows tokens
- typography follows tokens
- radius follows tokens
- colors follow tokens

---

# 76. Definition of Done — Main Editor

The main editor is ready when:

- every visible button has a defined action
- every icon-only control has a tooltip
- every menu has defined contents
- every dropdown has defined values
- every panel has empty/loading/error states
- every timeline interaction has defined mouse and keyboard behavior
- every selection has an explicit visual state
- every destructive action has recovery/confirmation
- Petroleum Blue is the only primary accent
- purple is absent from the product UI
- timeline semantic colors are consistent
- Inspector content is selection-aware
- keyboard shortcuts are documented and customizable
- panel resizing works
- undo/redo covers core editing operations

---

# 77. Core UX Rule

When implementing any new control, answer these six questions before coding:

1. What object does this control affect?
2. What happens on click?
3. What happens on hover?
4. What happens on keyboard focus?
5. What visual state proves that the action happened?
6. What can the user do to undo/reverse it?

If these six answers are not defined, the control is not ready for implementation.

---

# 78. Current Visual Baseline

The approved visual baseline is the current oca editor screenshot updated to the Petroleum Blue direction.

The current composition is approved as the reference layout:

- compact top application bar
- left media browser
- central program monitor
- contextual inspector on right
- vertical tool rail
- dense bottom timeline
- right-side audio meters

The structure should not be redesigned without an explicit product/design decision.


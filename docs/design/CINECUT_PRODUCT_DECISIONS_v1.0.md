# oca — Product Decisions: Remaining UI/UX Features

## Status

These decisions are FINAL for the current implementation scope.

Do NOT invent alternative interaction models for these features.
Do NOT expand the scope beyond what is explicitly defined below.
Do NOT implement features marked as "Out of Scope / Future".

The goal is to allow implementation to continue without requiring additional product decisions.

---

# 1. General Product Rule

The UI/UX specification must NOT be interpreted as a requirement to implement every feature mentioned in the document.

A distinction must always exist between:

- `IMPLEMENTED` — feature exists and must be fully implemented.
- `PLANNED` — approved future feature, but not part of the current implementation.
- `OUT OF SCOPE` — do not implement.
- `PRODUCT DECISION REQUIRED` — do not invent behavior.

For the current items described in this document, all decisions below are final.

---

# 2. Toolbar — Final Tool Set

The oca toolbar currently contains 7 tools.

The final toolbar for the current version is:

1. Selection
2. Razor
3. Trim
4. Text
5. Effects
6. Hand
7. Zoom

Do NOT add an eighth tool simply to match an earlier written specification.

Tools represent direct manipulation modes, not every capability available in the editor.

A feature does not necessarily require its own toolbar tool.

---

# 3. Selection Tool

Shortcut:

`V`

This is the default tool.

## Behavior

The Selection Tool is used to:

- select timeline clips;
- select multiple clips;
- move clips;
- resize/trim clip edges when using the appropriate edge interaction;
- select graphics;
- select objects in the Program Monitor;
- select Inspector properties where applicable;
- interact with timeline content.

## Activation

Click the Selection Tool.

The icon becomes active using Petroleum Blue.

Pressing `V` activates it.

## Selection

Single click:

- selects the clicked object;
- clears previous selection unless modifier selection is active.

Shift + click:

- adds/removes object from selection.

---

# 4. Razor Tool

Shortcut:

`C`

## Activation

Click Razor Tool or press `C`.

## Timeline behavior

Clicking a clip at a timeline position splits the clip at that position.

Example:

```text
Before:

┌───────────────────────────────┐
│          Clip.mp4             │
└───────────────────────────────┘
                 ↑
              cursor

After:

┌────────────────┐ ┌─────────────┐
│    Clip.mp4    │ │  Clip.mp4   │
└────────────────┘ └─────────────┘
```

---

# 5. Trim Tool

## Activation

Click Trim Tool.

## Behavior

The Trim Tool is used for direct timeline edge trimming.

When hovering over a clip edge:

- show trim cursor;
- highlight the editable edge.

Dragging the edge:

- changes clip duration;
- updates the edit point;
- updates timeline content.

While trimming, show useful technical information:

- source timecode;
- sequence timecode;
- trim duration.

Release commits the edit.

Undo must restore the previous trim.

---

# 6. TEXT TOOL

Shortcut:

`T`

## Purpose

The Text Tool creates Text Graphics directly in the Program Monitor.

It does NOT create a separate external text editor.

## Activation

Click Text Tool or press `T`.

The tool becomes active using Petroleum Blue.

Tooltip:

`Text Tool — T`

## Creating text

With Text Tool active:

### Single click in Program Monitor

Clicking any point inside the Program Monitor creates a new Text Graphic at that position.

Do NOT require click-and-drag to create a text box in the current version.

The initial text content should be editable immediately.

Example:

```text
┌─────────────────────────────────┐
│                                 │
│                                 │
│            [TEXT]               │
│              ↑                  │
│            click                │
│                                 │
└─────────────────────────────────┘
```

After creation:

- Create Text Graphic.
- Position it at clicked coordinates.
- Select it.
- Focus the text/content input.
- Allow the user to immediately type.

### Empty text

If the user presses Escape before entering any content:

- cancel the creation;
- remove the empty Text Graphic.

If text already contains content:

- Escape exits text editing;
- the graphic remains.

## Selecting existing text

With Text Tool active:

- Clicking an existing text graphic selects it.
- Inspector switches to Text properties.

## Moving text

Dragging the selected text graphic inside the Program Monitor moves its position.

This modifies the graphic's position.

It does NOT modify the timeline position.

## Exiting Text Tool

Press:

`V`

to return to Selection Tool.

---

# 7. TEXT INSPECTOR

When a Text Graphic is selected, the Inspector becomes contextual.

Minimum sections:

```text
TEXT
────────────────────────
Content
[____________________]

FONT
Family
Size
Weight

ALIGNMENT
Left
Center
Right

POSITION
X
Y

APPEARANCE
Color
Opacity
```

Additional typography features can be added later, but they are not required for the current implementation unless already present in the application.

Do not invent a complex typography system.

---

# 8. EFFECTS TOOL

## Important Product Decision

The Effects Tool does NOT mean:

"Click somewhere in the Program Monitor to place an effect."

That behavior is explicitly NOT desired.

Effects are clip-based.

## Activation

Click Effects Tool.

The tool:

- activates the Effects mode;
- opens or focuses the Effects Panel.

Tooltip:

`Effects — `

No direct painting/application behavior is required.

---

# 9. EFFECTS PANEL

Effects are organized into categories.

Suggested categories:

- Blur & Sharpen
- Color
- Distortion
- Keying
- Light
- Stylize
- Transform
- Utility

Only categories supported by the implementation should be shown.

Do not create fake categories simply to fill the UI.

---

# 10. Applying an Effect

There are exactly two primary ways to apply an effect.

## Method A — Double-click

1. Select a clip in the timeline.
2. Open/focus Effects Panel.
3. Find an effect.
4. Double-click the effect.

Result:

- effect is applied to the selected clip;
- Inspector updates to show the effect;
- effect becomes part of the clip's effect stack.

## Method B — Drag and Drop

1. Drag an effect from the Effects Panel.
2. Drop it onto a timeline clip.

While dragging over a valid clip:

- highlight the target clip;
- show that the effect can be applied.

On release:

- apply the effect to the target clip;
- update Inspector.

---

# 11. Applying Effect With No Selection

If the user double-clicks an effect and no clip is selected:

Do NOT apply the effect anywhere.

The Effects Panel should communicate:

`Select a clip to apply an effect.`

The user can continue browsing/searching effects.

---

# 12. Effects Tool — Explicit Non-Goals

The Effects Tool does NOT:

- paint effects onto the Program Monitor;
- create effect regions;
- draw masks;
- track objects;
- apply effects based on screen coordinates.

Masks/tracking/effect regions are future features.

---

# 13. ZOOM TOOL

## Purpose

The Zoom Tool controls the Program Monitor viewport.

It changes how the video is displayed to the user.

It does NOT modify the actual clip.

## Critical distinction

Zoom Tool

Changes:

- Program Monitor viewport zoom

Inspector → Scale

Changes:

- Clip Transform → Scale

These must never be conflated.

---

# 14. Zoom Tool Behavior

Activate Zoom Tool by:

- clicking the tool;
- pressing its assigned shortcut if implemented.

## Program Monitor click

Single click:

- zooms in one step around the cursor position.

## Alt/Option + click

Zooms out one step around the cursor position.

## Mouse wheel

Scrolling over Program Monitor changes zoom continuously.

Scroll up:

- zoom in.

Scroll down:

- zoom out.

## Double click

Double-clicking the Program Monitor while Zoom Tool is active returns to:

- Fit

## Pan

Zoom Tool does NOT pan.

The Hand Tool is responsible for panning.

---

# 15. Zoom Tool — Explicit Non-Goals

Zoom Tool must never modify:

- Scale
- Position
- Rotation
- Anchor
- Crop
- any rendered clip property

It is strictly a viewport operation.

---

# 16. EXPORT PROGRESS

## Product Decision

Export progress must be implemented.

However, the current version does NOT require estimated remaining time.

Do NOT implement ETA prediction.

---

# 17. Export Progress UI

During export, show:

```text
EXPORTING
██████████████░░░░░░
68%
00:01:42 elapsed
Interview_Final.mp4
[Cancel]
```

Required information:

- export state;
- progress percentage;
- progress bar;
- elapsed time;
- output filename;
- Cancel action.

---

# 18. Export ETA

Estimated remaining time is explicitly OUT OF SCOPE for the current version.

Do NOT display:

`Estimated remaining: 00:00:47`

Do not create an ETA algorithm merely to satisfy the UI specification.

ETA can be added later when the export pipeline exposes reliable timing information.

---

# 19. Export Core/UI Contract

The UI should consume high-level export events.

Conceptually:

```text
ExportStarted
ExportProgress
ExportCompleted
ExportFailed
ExportCancelled
```

ExportProgress should expose at minimum:

```text
progress: 0.0 .. 1.0
```

Elapsed time may be exposed by the core/export layer if convenient.

The UI must NOT depend on encoder-specific implementation details.

Do not leak codec/encoder internals into UI state.

---

# 20. Export Progress States

## Starting

```text
EXPORTING
Preparing...
```

## In progress

```text
EXPORTING
██████████░░░░░░░░
52%
00:00:41 elapsed
Interview_Final.mp4
[Cancel]
```

## Complete

```text
EXPORT COMPLETE
Interview_Final.mp4
[Open File] [Reveal in Folder] [Close]
```

## Failed

```text
EXPORT FAILED
The export could not be completed.
[Retry] [Open Settings] [Close]
```

## Cancelled

```text
EXPORT CANCELLED
The export was cancelled.
[Close]
```

---

# 21. KEYFRAMES

## Product Decision

Do NOT introduce a new timeline-based individual-keyframe interaction model in the current version.

The current CINECUT model is property-driven.

Keep it.

---

# 22. Current Keyframe Model

Keyframes are controlled through Inspector property controls.

Example:

```text
TRANSFORM
Position
X  960.0
Y  540.0                         ◇
```

The diamond is the keyframe control.

## Creating a keyframe

1. Move playhead to desired time.
2. Select property.
3. Click keyframe diamond.

Result:

- keyframe is created at current playhead position.

## Existing keyframe

If a keyframe already exists at the current playhead:

The UI must clearly indicate that the current property has a keyframe at this time.

Follow the existing application's current toggle/update semantics.

Do not create a second keyframe at exactly the same timestamp.

---

# 23. Keyframe Timeline Interaction — OUT OF SCOPE

Do NOT add the following unless explicitly approved later:

- individual keyframe selection in the timeline;
- dragging keyframes horizontally;
- keyframe context menus;
- Bezier handles;
- Hold interpolation;
- Ease In;
- Ease Out;
- timeline keyframe editing tools.

The current implementation remains:

```text
Playhead
   ↓
Inspector Property
   ↓
Keyframe Diamond
```

---

# 24. PROGRAM MONITOR OVERLAYS

## Product Decision

The following are NOT part of the current implementation:

- Safe Areas
- Grid
- Guides

They are future features.

---

# 25. Program Monitor "..." Menu

Do NOT show controls for features that do not exist.

The menu must only contain currently implemented functionality.

Therefore, do NOT add:

```text
☐ Safe Areas
☐ Grid
☐ Guides
```

just because they appeared in an earlier design specification.

---

# 26. Future Overlay Architecture

When overlays are implemented in the future, they may live under:

```text
View
 └── Program Monitor Overlays
      ├── Safe Areas
      ├── Grid
      ├── Guides
      ├── Metadata
      └── Camera Overlay
```

But these items should remain hidden until their corresponding functionality actually exists.

---

# 27. Program Monitor — Current Scope

The current Program Monitor continues to support:

- zoom percentage;
- Fit;
- Fill;
- Actual Size;
- playback;
- timeline scrubbing;
- implemented metadata overlays;
- implemented technical overlays.

Do not create placeholder functionality.

---

# 28. UI SPECIFICATION VS FEATURE SCOPE

This distinction is critical.

A UI specification can describe how a future feature SHOULD behave.

That does NOT mean the feature belongs in the current implementation.

Before implementing a UI element, determine:

```text
Does the feature exist?
        │
        ├── YES → implement according to spec
        │
        └── NO
             │
             ├── marked FUTURE → do not implement
             │
             └── PRODUCT DECISION REQUIRED
                    → stop and ask
```

---

# 29. No Invented UX

Do NOT independently decide:

- what an unspecified tool should do;
- how an unspecified interaction should work;
- what a missing feature should look like;
- what a new timeline interaction should be;
- what a new core event should contain.

If behavior is not defined by this document or the existing application architecture:

STOP and request a product decision.

Do not silently invent behavior.

---

# 30. Current Product Decisions Summary

| Feature | Decision | Current Scope |
|---|---|---|
| Selection Tool | Standard selection/editing | IMPLEMENT |
| Razor Tool | Click clip to split | IMPLEMENT |
| Trim Tool | Direct edge trimming | IMPLEMENT |
| Text Tool | Click monitor → create text | IMPLEMENT |
| Effects Tool | Focus Effects Panel | IMPLEMENT |
| Effect application | Double-click or drag/drop onto clip | IMPLEMENT |
| Zoom Tool | Program Monitor viewport zoom | IMPLEMENT |
| Export progress | Percentage + progress bar + elapsed | IMPLEMENT |
| Export ETA | Estimated remaining time | FUTURE |
| Keyframe creation | Inspector diamond at playhead | KEEP CURRENT MODEL |
| Timeline keyframe selection | Individual timeline interaction | OUT OF SCOPE |
| Safe Areas | Monitor overlay | FUTURE |
| Grid | Monitor overlay | FUTURE |
| Guides | Monitor overlay | FUTURE |

---

# 31. Final UX Rule

CINECUT should favor predictable, professional editing behavior over feature density.

The user should always understand:

1. What object is being affected.
2. What the current tool does.
3. What will happen when they click.
4. What changed after the interaction.
5. How to undo it.

Do not introduce interaction patterns merely because they are technically possible.

When in doubt, prefer the simplest behavior that is consistent with professional desktop video editors and the existing CINECUT architecture.

---

# 32. Implementation Status

After applying these decisions:

## Approved for implementation

- Text Tool
- Effects Tool
- Effects application
- Zoom Tool
- Export progress
- Existing Inspector-driven keyframes

## Explicitly deferred

- Export ETA
- Timeline individual keyframe manipulation
- Safe Areas
- Grid
- Guides
- Mask-based effect placement
- Effect painting/regions
- Advanced text-box creation
- Advanced keyframe interpolation

These deferred features must NOT block the current implementation.

The implementation should continue with the approved scope above.

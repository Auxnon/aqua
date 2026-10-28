# App layout guideline

Aqua runs the same Rust engine on every platform (Android today, the web, and
later iOS or desktop). Each platform builds its own UI with its own controls,
and buttons may look native to the platform. What every version keeps the same
is where things are, what they do, and how the painting responds. Someone who
paints on the phone should find their way around the web version without
thinking.

## Screens

There are two screens.

**Gallery.** The title "Aqua" at the top left. Saved paintings sit in a grid of
cards, newest first. Each card shows the thumbnail at the painting's own aspect
ratio, with the date under it. A single primary button at the bottom right
starts a new painting. A card's secondary action opens a menu with Duplicate and
Delete. On Android that action is a long-press; on the web it is a right-click
or a long-press. Delete asks for confirmation.

**Editor.** The painting fills the screen. The controls float over it in fixed
zones.

## Editor zones

```
┌──────────────────────────────────────────────┐
│ (← ↶ ↷)                    (dry ◎ ⇪ ≣ ●)     │  top bar: two pill groups
│                                   ┌───────┐  │
│ ┃                                 │ panel │  │  panels drop from the top right
│ ┃ size                            └───────┘  │
│ ┃                                            │
│ ┃ water / bleed / strength                   │  side rail, vertically centred
│                                              │
│                [ brush] panel                │  brush panel sits above the dock
│        (brush · water · ink · eraser · pick) │  tool dock, bottom centre
└──────────────────────────────────────────────┘
```

- **Top left: navigation and history.** Back to the gallery, undo, redo.
- **Top right: actions, in this order.**
  1. Dry layer now. Only shown while the active layer is wet.
  2. Gimbal (tip the paper).
  3. Export.
  4. Layers.
  5. The current colour as a swatch, which opens the palette.
- **Left rail.** Brush size on a log scale, then the current tool's main
  control: water for the watercolour and water brushes, bleed for ink, strength
  for the eraser. Rail drags are relative, so grabbing a slider never makes it
  jump.
- **Bottom dock.** Watercolour, clear water, ink, eraser and eyedropper, in that
  order. Tapping the selected tool again opens its brush panel.
- **Panels.** Only one is open at a time. Layers and Palette drop down from
  under the top-right group, and the brush panel sits above the dock. Touching
  the canvas closes any open panel.
- **Overlays.** The gimbal opens as a modal in the middle of the screen, and
  tapping outside it closes it. While a pinch would leave for the gallery, a
  "Release for gallery" pill appears at the bottom centre.

## Gestures and input

| Input | Does |
|---|---|
| One finger, pen or mouse | Paints with the selected tool |
| Two fingers | Pan, zoom and rotate together (rotation snaps to 0° within 4°) |
| Two-finger tap / three-finger tap | Undo / redo |
| Pen seen once | From then on fingers only navigate (palm rejection) |
| Pinch far out (below 45% of the fitted size) and release | Back to the gallery. The chrome fades and the painting flies into its card. |
| Back (system back, Escape) | Closes the gimbal, then any panel, then leaves for the gallery |

Desktop versions add the following: the mouse wheel zooms at the cursor; the
middle button, the right button or space + drag pans; Ctrl/⌘+Z undoes and
Shift+Ctrl/⌘+Z or Ctrl+Y redoes; `[` and `]` change the brush size;
B / W / I / E / K pick a tool; 0 fits the painting to the screen.

## Behaviour every version shares

- **Brush defaults and the parameter array** come from the engine
  (`BrushSettings::from_params`), and the per-tool defaults match.
- **Picking a colour while on the eraser or water** switches to the
  watercolour brush.
- **Palette.** The mixing tray has three modes: Paint, Mix and Pick. Mixed
  colours that are picked up are remembered, up to 16. The tray is kept between
  sessions.
- **Tipping the paper.** The tilt is set in screen space and converted to
  canvas space with the view's rotation. Phone tilt is measured from the pose
  when it started. "Keep phone tilt on when closed" only appears where a tilt
  sensor exists, and it pauses while the app is in the background.
- **Saving.** The painting is saved on leaving the editor, when the app is
  backgrounded, and (on the web) every 30 seconds while it changes. A painting
  is the same set of files everywhere: `meta.json`, `layer_<id>.bin` and
  `thumb.png`.
- **No blinks.** The canvas, its surround and the window background are all
  #09090B. The painting is only closed in the engine after the gallery is on
  screen.

## Tokens

| Token | Value | Use |
|---|---|---|
| Background | `#09090B` | Window, canvas surround |
| Panel | `#1A1A1E` at 90% | Bars, rail, dock, panels |
| Raised | `#2A2A30` | Chips, slider tracks, card placeholder |
| Line | white at 20% | Panel and card borders |
| Text | `#ECEAE6` | |
| Text dim | `#9A9894` | Captions, slider labels, disabled icons |
| Accent | `#8FD3CB` | Selection, the primary button, slider fills |
| Danger | `#E5806F` | Delete |
| Tray | `#E6E4DF` | Mixing tray paper |

- **Shapes.** Bars and the dock are pills; the dock radius is 28. Panels have
  radius 18, cards 6 and chips 14.
- **Sizes.** Top-bar buttons are 44 and dock buttons 48. Icons are half the
  button size.
- **Icons** are outlined, using each platform's own set or the web build's line
  icons. The eraser and the gimbal are custom glyphs on every platform.

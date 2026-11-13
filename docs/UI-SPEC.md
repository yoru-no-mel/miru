# miru — UI Specification

Honeyview-inspired layout: minimal dark canvas, chrome that gets out of the way.
Reference: screenshot of Honeyview 5.53 (pink theme, floating pill hotbar).

## Layout

```
+--------------------------------------------------------------+
| [icon] miru            [─] [□] [✕]              <- top bar |
|  View  Slideshow  Bookmark  Edit  Copy to  Lock        <- menu row |
+--------------------------------------------------------------+
|                                                              |
|                                                              |
|                        image canvas                          |
|                  (fit / 1:1 / free zoom)                      |
|                                                              |
|                                                              |
|                       +--------------+                        |
|                       | ====seek==== |  <- seekbar strip      |
|                       | [≡][+][−][⟲][⟳][Lock]  [‹] [›] |      |
|                       +--------------+  <- hotbar pill       |
+--------------------------------------------------------------+
```

- **Top bar**: titlebar (drag region, icon, title, window buttons) + menu row.
  Auto-hides.
- **Canvas**: full-window dark surface (`#1c1c1c`), image centered; empty state
  shows a faint placeholder mark in the center.
- **Hotbar**: floating rounded pill, bottom-center, ~12 px from the bottom edge.
  Seekbar strip sits directly above the button row inside the pill. Auto-hides.

## Theme tokens

| Token             | Value     | Use                                    |
|-------------------|-----------|----------------------------------------|
| `canvas_bg`       | `#1c1c1c` | Image background                       |
| `chrome_bg`       | `#2b2b2b` | Titlebar, menu row, hotbar pill        |
| `chrome_bg_hover` | `#3a3a3a` | Hovered chrome controls                |
| `accent`          | `#e05fa8` | Active/hover highlights (pink)         |
| `accent_soft`     | `#8c3b68` | Seekbar track fill                     |
| `text`            | `#e8e8e8` | Labels                                 |
| `text_dim`        | `#9a9a9a` | Secondary labels                       |
| `border`          | `#111111` | Pill / panel outline                   |

Window buttons: minimize `─`, maximize `□` / restore `❐`, close `✕` (close
hover = red `#e0475a`).

## Auto-hide behavior

Applies to the **top bar** and the **hotbar** alike.

**Reveal when any of:**

1. Pointer enters the reveal zone: top 48 px (top bar) or bottom 48 px
   (hotbar), measured from the window edge.
2. Pointer moves anywhere in the window after chrome was hidden (a single
   mouse-move reveals chrome — then the idle timer takes over).
3. Any keyboard shortcut that implies chrome interaction is pressed (e.g. a
   seekbar drag).
4. Chrome is **pinned** (Lock toggle active, or a "Pin UI" menu item).

**Hide when all of:**

1. Idle: no pointer movement for 1.5 s (configurable; 1.5–2.0 s target).
2. Pointer is not inside the chrome panel's rect.
3. Not pinned.

**Rules:**

- Top bar and hotbar track visibility independently (top can show while bottom
  hides).
- An in-progress interaction (dragging seekbar, holding a button, open menu
  popover) suspends hiding for that panel.
- During window resize or drag-to-move, both panels stay visible.
- In fullscreen (F11), behavior is identical — chrome is never "always on".
- Reveal/hide animates opacity + 8 px slide (120 ms ease-out); must not block
  input during animation.

## Top bar

- Height: titlebar 32 px + menu row 28 px.
- **Drag**: pressing and dragging any empty titlebar area moves the window.
- **Double-click** titlebar empty area toggles maximize/restore.
- **Window buttons**: 46 × 32 px hit targets, glyphs drawn (no OS chrome).
  - Minimize → iconify
  - Maximize/Restore → toggle viewport maximize
  - Close → exit (M5: confirm only if a slideshow is running)
- **Menu row** entries: `View`, `Slideshow`, `Bookmark`, `Edit`, `Copy to`,
  `Lock` (Lock is a toggle button with a check indicator, like the screenshot).
  v1: `View` popover hosts open/fit/zoom actions; the rest are disabled
  placeholders until their milestones.

## Hotbar

Floating pill, fixed height (~64 px incl. seekbar strip), width hugs content,
bottom-centered.

Button row (left → right):

| Control        | Glyph | Action                                        |
|----------------|-------|-----------------------------------------------|
| Menu           | `≡`   | Small popover (Open file/folder/archive, Fit)  |
| Zoom in        | `+`   | Zoom toward center, step 25% (min 10%)        |
| Zoom out       | `−`   | Zoom toward center, step 25%                  |
| Rotate CCW     | `⟲`   | Rotate −90°                                   |
| Rotate CW      | `⟳`   | Rotate +90°                                   |
| Lock           | `Lock`| Toggle: freeze zoom/pan/rotation across images |
| Previous       | `‹`   | Previous image in playlist                    |
| Next           | `›`   | Next image in playlist                        |

- Buttons: 36 × 36 px circular hit targets, icon-only, tooltip on hover
  (600 ms delay) with the shortcut key.
- At playlist bounds, `‹`/`›` are disabled (dimmed, no wrap in v1).
- All controls have keyboard focus + activation via Enter/Space.

## Seekbar

- Sits above the button row, inset 12 px each side, 6 px tall track,
  12 px tall grab target.
- Filled portion uses `accent_soft`; thumb is a small rounded marker
  (`accent` on hover/drag).
- **Semantics**: position in the current playlist (folder or archive).
- Click anywhere on the track → jump to that index (nearest image).
- Drag thumb → live scrubbing; the canvas updates on release (preview text
  `n / total` above the thumb while dragging).
- Hover tooltip: `image-name.webp  (12 / 73)`.
- Keyboard: focusable; Left/Right moves one index; Home/End jump to bounds.
- When the playlist has ≤ 1 image, the seekbar renders as a static empty track
  and is not interactive.

## Canvas interactions

| Input                    | Action                                          |
|--------------------------|-------------------------------------------------|
| Wheel up/down            | Zoom in/out at pointer                          |
| Drag (left button)       | Pan when zoomed beyond fit                      |
| Double-click canvas      | Toggle between fit-window and 100%              |
| Middle-drag              | Pan (even at fit)                               |

## Keyboard shortcuts

| Key          | Action                                    |
|--------------|-------------------------------------------|
| `←` `,`      | Previous image                            |
| `→` `.`      | Next image                                |
| `Home`       | First image                               |
| `End`        | Last image                                |
| `+` `=`      | Zoom in                                   |
| `-`          | Zoom out                                  |
| `0`          | Fit to window                             |
| `1`          | Zoom 100%                                 |
| `R`          | Rotate CW                                 |
| `Shift+R`    | Rotate CCW                                |
| `L`          | Toggle Lock                               |
| `F11`        | Toggle fullscreen                         |
| `Ctrl+O`     | Open file dialog                          |
| `Ctrl+Shift+O` | Open folder dialog                      |
| `Esc`        | Close open popover / exit fullscreen      |

## Empty state

Centered placeholder mark (faint geometric logo, 96 px) on the dark canvas with
a dim hint line: `Drop an image, folder, or archive  ·  Ctrl+O`.

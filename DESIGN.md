---
name: Retype
description: Change the extension. Keep the file.
colors:
  selection-yellow: "#ffe14a"
  ink: "#111418"
  ink-hover: "#2a2f36"
  soft: "#6a7079"
  line: "#e4e6ea"
  hover-wash: "#f1f2f4"
  paper: "#ffffff"
typography:
  display:
    fontFamily: "Hanken Grotesk, Helvetica Neue, Arial, sans-serif"
    fontSize: "clamp(2.2rem, 6vw, 4.2rem)"
    fontWeight: 700
    lineHeight: 1
    letterSpacing: "-0.035em"
  wordmark:
    fontFamily: "Hanken Grotesk, Helvetica Neue, Arial, sans-serif"
    fontSize: "clamp(2.2rem, 5vw, 3.4rem)"
    fontWeight: 700
    lineHeight: 1
    letterSpacing: "-0.03em"
  body:
    fontFamily: "Hanken Grotesk, Helvetica Neue, Arial, sans-serif"
    fontSize: "1rem"
    fontWeight: 400
    lineHeight: 1.5
  button:
    fontFamily: "Hanken Grotesk, Helvetica Neue, Arial, sans-serif"
    fontSize: "1rem"
    fontWeight: 600
    lineHeight: 1.5
  label:
    fontFamily: "Hanken Grotesk, Helvetica Neue, Arial, sans-serif"
    fontSize: "0.95rem"
    fontWeight: 600
    lineHeight: 1.5
  caption:
    fontFamily: "Hanken Grotesk, Helvetica Neue, Arial, sans-serif"
    fontSize: "0.86rem"
    fontWeight: 400
    lineHeight: 1.45
rounded:
  xs: "2px"
  sm: "3px"
  selection: "4px"
  md: "6px"
  lg: "8px"
  mark: "22.5%"
spacing:
  xs: "4px"
  sm: "10px"
  md: "14px"
  lg: "16px"
  xl: "18px"
  window: "clamp(18px, 3vw, 28px)"
components:
  extension-selection:
    backgroundColor: "{colors.selection-yellow}"
    textColor: "{colors.ink}"
    typography: "{typography.display}"
    rounded: "{rounded.selection}"
    padding: "0.04em 0.12em 0.08em"
  format-chip:
    backgroundColor: "transparent"
    textColor: "{colors.soft}"
    typography: "{typography.label}"
    rounded: "{rounded.md}"
    padding: "6px 10px"
  format-chip-hover:
    backgroundColor: "{colors.hover-wash}"
    textColor: "{colors.ink}"
  format-chip-selected:
    backgroundColor: "{colors.selection-yellow}"
    textColor: "{colors.ink}"
  button-primary:
    backgroundColor: "{colors.ink}"
    textColor: "{colors.paper}"
    typography: "{typography.button}"
    rounded: "{rounded.lg}"
    padding: "11px 16px"
  button-primary-hover:
    backgroundColor: "{colors.ink-hover}"
  progress-track:
    backgroundColor: "{colors.line}"
    rounded: "{rounded.xs}"
    height: "3px"
  progress-fill:
    backgroundColor: "{colors.ink}"
    rounded: "{rounded.xs}"
    height: "3px"
  status-filename:
    backgroundColor: "{colors.selection-yellow}"
    textColor: "{colors.ink}"
    rounded: "{rounded.sm}"
    padding: "0 3px"
---

# Design System: Retype

<!-- Source of truth: the Retype board in brand/exploration/round-2-restrained.html (--r-* tokens, .rt* rules, ICONS.retype, BRANDS.retype). The app is a native macOS desktop app on an undecided stack; values marked PROVISIONAL exist only because the source is a web mock. -->

## Overview

**Creative North Star: "The Selected Extension"**

Retype is a white page, black ink, and one yellow: the colour of selected text. The whole interface is a file name set large in bold, with the part after the dot highlighted as if you had just selected it to type a new one. Moving through formats (scroll wheel or arrow keys) swaps the highlighted extension in place; one click saves. The brand is that moment, and nothing is added on top of it.

The system is restrained by construction. One typeface, two neutrals plus a hairline grey, one accent. No gradients, no shadows inside the window, no illustration, no metaphor costume. Density is low: a single column of four rows (source line, file name, format list and action, progress and status) inside a plain window.

Yellow means "selected". Wherever it appears (the current extension, the chosen chip, the file name in the done message, the system text selection, the mark) it marks the text that is chosen or about to change. It never decorates.

**Key Characteristics:**
- White paper, ink text, one selection-yellow accent.
- One family, Hanken Grotesk; bold for the file name, 600 for controls, regular for prose.
- The file name is the display type; the extension is the picker.
- Lowercase extensions with their dot, everywhere.
- Flat: hairlines and the yellow fill carry all structure.

## Colors

A near-monochrome palette of cool white and blue-black ink, with a single warm yellow lifted straight from the text-selection highlight.

### Primary
- **Selection Yellow** (`selection-yellow`): the highlight behind the current extension, the selected format chip, the file name in the done message, and the global text-selection colour (`::selection`). Always a fill behind ink text, never a text or line colour.

### Neutral
- **Ink** (`ink`): all primary text, the file name, the primary button fill, the progress fill, the dot and caret in the mark. Also the text colour on yellow (the source's `--r-sel-ink` equals ink).
- **Ink Lifted** (`ink-hover`): primary button hover only.
- **Soft Graphite** (`soft`): secondary text: the source line, the tagline, unselected format chips, the quality tag, fine print. Passes 4.9:1 on paper.
- **Hairline** (`line`): progress track, dividers, the mark's outline stroke.
- **Hover Wash** (`hover-wash`): background of a format chip under the pointer.
- **Paper** (`paper`): the window and page background; button text on ink.

### Dark
The app follows the system appearance. The yellow and the ink on it do not change; everything else does.

| Token | Light | Dark |
|---|---|---|
| paper | #ffffff | #17191d |
| ink (text, primary button fill) | #111418 | #eceef1 |
| ink-hover | #2a2f36 | #d2d6dc |
| soft | #6a7079 (4.9:1) | #969da7 (6.3:1) |
| neighbour | #8a9098 | #6c737d |
| line | #e4e6ea | #2e3239 |
| hover-wash | #f1f2f4 | #25282e |
| selection-yellow | #ffe14a | #ffe14a |
| on-yellow | #111418 | #111418 |

The primary button inverts with the theme: ink fill and paper text, so it is near-white with dark text in dark mode.

### Named Rules
**The Selection Rule.** Yellow appears only behind text that is selected, chosen, or just produced. If nothing on screen is selected, no yellow shows except the mark.

**The Fill-Not-Ink Rule.** Yellow never sets text or strokes. On paper it is 1.3:1; it only works as a fill behind ink (14.2:1). One exception, by request: the drop-zone outline, a moving yellow dash that marks the card as a target. It carries no information a reader must decode, so contrast is not the job; motion and colour are.

## Typography

**Display Font:** Hanken Grotesk (with Helvetica Neue, Arial, sans-serif)
**Body Font:** Hanken Grotesk (same family)

**Character:** One grotesque in three weights. Bold, tightly tracked display for the file name; semibold for anything you can press; regular for sentences.

### Hierarchy
- **Display** (700, clamp(2.2rem, 6vw, 4.2rem), line-height 1, -0.035em): the file name and its highlighted extension. One per window.
- **Wordmark** (700, clamp(2.2rem, 5vw, 3.4rem), line-height 1, -0.03em): the name "Retype" set as type next to the mark.
- **Body** (400, 1rem, 1.5): prose, the tagline (at 1.1rem in soft), status lines (0.95rem), the source line (0.9rem, soft).
- **Button** (600, 1rem): the primary action label.
- **Label** (600, 0.95rem): format chips.
- **Caption** (400, 0.86rem, 1.45): the quality tag and fine print, in soft.
- **Neighbour extensions** (600, 0.42em of the display size, -0.01em): the previous and next extension stacked above and below the highlight. See Components for the colour caveat.

Clamp sizes are web-mock values. PROVISIONAL: the native app needs fixed point sizes per window size; keep the ratios (display about 4x body, neighbours at 0.42 of display).

### Named Rules
**The One Family Rule.** Hanken Grotesk only. Hierarchy comes from size, weight (400 / 600 / 700) and tracking, never from a second face.

**The Lowercase Dot Rule.** Extensions are written lowercase with their leading dot (".mov", "Save as .mov"). In the display line the dot is its own glyph and the highlight covers only the letters after it.

PROVISIONAL: the mock loads Hanken Grotesk from Google Fonts (400 to 800). The native app must bundle the font files (OFL) and needs only 400, 600 and 700.

## Layout

The app window holds one column with an 18px gap between rows and window padding of clamp(18px, 3vw, 28px) (PROVISIONAL: fix to a native value near 24px). Rows, top to bottom: source line ("From interview.mp4"), the display line, the format chips 14px below it, the action row 16px below that (button plus quality tag, 10px row and 16px column gaps), then a 3px progress bar and a single status line. Minimum content height in the mock is 330px.

The display line is a single row that never wraps. The file name truncates with an ellipsis; the extension never does. Format chips wrap with a 4px gap.

### Named Rules
**The Extension Never Truncates Rule.** When space runs out, the file name gives way with an ellipsis. The dot and the highlighted extension always stay whole and visible.

## Window

A floating card, not a framed window: no title bar, 14px corners, a 1px hairline ring, the system shadow behind it, and on top of other windows by default (pin to change). The background drags the window. Pin and close sit top right at 28px; on Pick, Working and Done they share the row with the source line, which starts with a back arrow on Pick and Working (start over; Esc) and a plus on Done (new file; Esc). Back navigation does the same as Esc on every screen: the mouse Back button, ⌘[ on macOS, Alt+← on Windows. Notices clear after 3s (problems after 6s) and the card shrinks back.

The card fits its content in both directions and springs between sizes. Width: wide enough for the whole file name with the widest extension in its list, and for the chips on one row, between 420 and 640. Past 640 the name truncates in the middle and the chips wrap into rows of even length. Widths are measured against the widest extension, so neither the name nor the card moves while scrolling through formats. Height: exactly the content. There is no reserved empty space: status lines appear only when there is something to say, and push the card taller.

### Settings
A list on the same card: label left in body type, switch right, 38px rows, a hover wash on the whole row (the row is the control). Switch: 34×20 pill; on is a selection-yellow track with an on-yellow knob, off is a hairline track with a paper knob. Labels say what is kept or done ("Keep location"), never "Enable…". The longer explanation lives in the row's tooltip. Opened from the gear on the empty card (which replaced the right-click toggle there; that toggle is now a row).

### Focus
Keyboard focus draws a 2px ink ring 2px outside the control. The card background is not focusable.

### Drop zone
While the card waits for a file (Empty), a 2px yellow dashed outline runs 9px inside its edge (period ~14px, 57% dash, fitted so a whole number of dashes closes the loop; the path starts mid-top so there is no seam), travelling at 22px/s. While files are dragged over the card, on any screen, it thickens to 3px and speeds to 60px/s, and the card takes a faint yellow tint (8% light, 4.5% dark). Nothing else changes on drag-over: no preview, no resize. Reduce Motion stops the dashes moving.

On Done the outline returns once the entrance has settled (fades in from 0.8s to 1.3s), at 55% opacity and 12px/s: the result stays the focus, but the card still says "drop the next one here". The source row's leading icon on Done is a plus ("New file", Esc), not a back arrow: finishing is a step forward, not an undo.

**Arrival feedback.** When files first enter the card: a light trackpad tick (macOS `NSHapticFeedbackPattern.alignment`, felt on Force Touch trackpads because the finger is down mid-drag) and the card surface squeezes 6px and springs back with a wobble. On drop: a firmer tick (`levelChange`) and a smaller outward bounce. Content does not move; only the surface does.

## Motion

Springs, not tweens: stiff and lightly underdamped, settling in about 250ms with a small overshoot. Reduce Motion turns every spring into a snap.

- **Reel:** the extension springs in from the previous slot (with a little bounce); the neighbours move with it.
- **Highlight width:** the yellow behind the extension springs to the new extension's width.
- **Chip selection:** snaps. The reel above is the moving part; the chips are the index.
- **Done entrance:** the tile pops from 82% to full size, and the yellow sweeps across the new file name left to right, as if it were just selected.
- **Press:** buttons and icon buttons spring to 94% while held.
- **Toggles:** the yellow fill (or the on-dot) pops in on a spring.
- **Card size:** the card springs inside the window. The OS window is resized at most once per change: immediately when growing (the card then springs out into it) and once at the end when shrinking. Live window resizes stutter; never animate the window itself.
- **Progress, toast slide-in:** springs.

## Elevation & Depth

Flat. Inside the window there are no shadows; structure comes from the hairline grey, the hover wash, and the yellow fill. The mock draws its window with a 1px hairline ring and a soft drop shadow; that stands in for the OS window and is not a brand surface. In the native app the system supplies window chrome and shadow.

### Named Rules
**The Flat Window Rule.** Nothing inside the card casts a shadow; only the card itself floats. Selection is shown by the yellow fill, hover by the wash.

## Shapes

Small, quiet radii that scale with the element: 2px on the progress bar, 3px on the inline file-name highlight, 4px on the display highlight (so it reads like a text selection, not a pill), 6px on chips, 8px on the button. The mark is a rounded square at 22.5% radius with a 1px hairline outline.

The mark: a white rounded square; an ink dot at lower left; a yellow block (the selected extension, 5-unit radius on a 100 grid) right of it; an ink text caret at the block's right edge. The dot always sits in the mark so the name does not read as a font tool. Shown at 76, 40 and 22px. Source: `assets/icon.svg`. PROVISIONAL: the macOS app icon needs the platform icon grid and its own size variants; this SVG is the drawing, not the shipping icon.

## Components

### Extension Picker (signature)
The file name in display type, a plain dot, then the current extension in a yellow highlight with ink text. A thin ink caret (0.05em wide) sits just right of the highlight. The previous and next extensions sit small above and below the highlight in a three-row stack, so the list reads as a vertical reel you scroll through. Scroll wheel (one step per 45px of delta), arrow keys (all four) step through the list with wrap-around; Enter converts. Clicking the highlighted extension converts.

- PROVISIONAL: the caret blinks (1.1s, hard step, half on, half off) in the mock. Native should follow the system caret blink and stop under Reduce Motion, which the mock already does.
- The neighbour extensions are decorative and repeated by the chips. The app sets them in #8a9098 (3.3:1 on paper, large semibold text). The mock's #b9bdc4 (1.9:1) is retired.

### Format Chips
- **Style:** no fill, soft text, 600 label type, ".ext" lowercase, 6px radius, 6px 10px padding, 4px apart.
- **Hover:** hover-wash fill, ink text.
- **Selected:** yellow fill, ink text. Marked `aria-current`.
- **Behaviour:** clicking a chip selects it and converts immediately. One click is the whole job.

### Buttons
- **Shape:** 8px radius.
- **Primary:** ink fill, paper text, 600, 11px 16px padding. Label is "Save as .ext" and follows the current selection.
- **Hover:** ink-hover fill. No transform, no shadow.
- **Focus:** PROVISIONAL. The mock inherits a 2px blue outline from the exploration page shell. Native uses the system focus ring.

### Progress and Status
- 3px bar, hairline track, ink fill, 2px radius. Fill eases on `cubic-bezier(.3, .6, .2, 1)`. Durations in the mock (450ms remux, 1500ms re-encode) are simulated; native shows real progress.
- One status line below, body type at 0.95rem. In the done message the saved file name sits in a yellow highlight (3px radius, 0 3px padding, 600).

### Drop State
While a file is dragged over the window, a 2px dashed outline in the current text colour at 50% opacity sits 8px inside the content edge, 8px radius. PROVISIONAL: shared mock scaffold; confirm against native drag feedback.

### Icons
Lucide 1.60.0 (ISC), bundled as `assets/fonts/lucide.ttf` and drawn as text. One stroke weight, one family; no emoji or Unicode stand-ins. Icons replace explanatory text: what an icon does is said in its tooltip (a paper card with a hairline, body type, the shortcut after two spaces), not in a line on screen.

- **Icon button:** 36px square (28px in the title row), 8px radius, soft icon at half the button size. Hover: hover-wash fill, ink icon. On, for a choice that applies to the next conversion (Smaller, Replace, Copied): selection-yellow fill, on-yellow icon.
- **Setting toggle** (pin, right-click menu): stays on across conversions, so it is quieter: ink icon with a 6px yellow dot at the lower right when on, soft icon when off.
- **Kind icon** before the source line: file-video, file-music, file-image, file-text (subtitles), file-braces (data), files (mixed).
- **Quality badge** after the actions, caption 600 soft: zap "Lossless", hourglass "Slow". Nothing when a conversion is an ordinary re-encode.
- **Notices** (errors, Trash, Downloads) are one line led by an icon: circle-alert, trash-2, download, check.

### Copy
Plain, short, no slogans, and only where an icon can't carry it.
- Empty: "Drop a file." with one icon row below: folder-open (choose) and clipboard-paste (paste) on the left, the right-click-menu toggle on the right. No privacy badge: an icon that does nothing reads as a broken button, and people install Retype already knowing it is local.
- Pick: kind icon + "interview.mp4 · 32 KB"; the button names the action and follows the toggles: "Save as .jpg", "Save smaller .jpg", "Replace with .jpg"; toggles shrink (⇧) and trash-2 (⌥).
- Working: the bar, "62%" or "4 of 12", an × to cancel.
- Done: the file row, its size, "Open", then folder (reveal), copy (turns into check for 1.5s), arrow-left-right (another format), undo-2. Status line only for exceptions: "Original in the Trash", "Saved to Downloads (that folder is read-only)", failures.
- Unsupported: "Can't convert .xyz yet."

## Do's and Don'ts

### Do:
- **Do** keep yellow (`selection-yellow`) to selected or just-produced text: the current extension, the chosen chip, the saved file name, text selection.
- **Do** set the file name in 700 display type and let it truncate before the extension does.
- **Do** write extensions lowercase with their dot.
- **Do** use soft graphite for anything secondary and ink for anything the user acts on.
- **Do** make every format reachable by scroll wheel and arrow keys, and conversion one click or Enter.
- **Do** write status lines as plain past-tense facts ("Saved interview.mov next to the original.").

### Don't:
- **Don't** use yellow as text, borders, or a background panel; it is a fill behind ink only.
- **Don't** add a second typeface, a monospace, or uppercase extensions.
- **Don't** put shadows, gradients, or illustration inside the window.
- **Don't** use a converter-arrows icon or any arrow-between-files motif; the mark is the dot, the selection, and the caret.
- **Don't** write slogans or buzzwords in the UI ("blazing fast", "seamless", "magic").

---
description: "Distilled Impeccable Operate/Polish rules for lightweight HTML/CSS product UI"
---
MODE: Operate / Read (precision and restraint; the tool disappears into the task).

1. HIERARCHY & CHROME:
   - Reserve bordered pills/cards for interactive controls or primary status; render passive telemetry (git, tokens, t/s) as quiet inline text with `tabular-nums`.
   - Use two neutral surface layers (`--bg` canvas, `--panel` chrome) plus `--card` for elevated inputs; avoid nesting boxed borders inside boxed borders.
   - Use `--accent` only for active selection, primary send/action, and focus states—never decoration.

2. TYPOGRAPHY & CONTRAST:
   - Pair one clean sans (`--sans`) for labels and readable `65–72ch` prose with `--mono` strictly for code, paths, diffs, and metrics.
   - Tight fixed type scale (11px section caps, 12px chrome, 13.5px prose); never use fluid `clamp()` headings in app shells.
   - Body, labels, and placeholders must hold >= 4.5:1 contrast against their surface across both dark and light themes.

3. INTERACTION & SLOP GUARD:
   - Native-feeling, instant transitions (`120–150ms`); never add `filter: blur()` glow hacks, decorative icon tiles per list item, or gratuitous motion.
   - Keep zero external CSS/JS dependencies and preserve existing CSS theme tokens (`--bg`, `--panel`, `--card`, `--border`, `--text`, `--muted`, `--accent`).

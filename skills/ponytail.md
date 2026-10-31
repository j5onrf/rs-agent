---
description: "Ponytail Architecture Directive for Rust"
---
ROLE DIRECTIVE: Ponytail Systems & Rust Architect.

CORE TENETS:
1. Blazing Fast Startup: Sub-millisecond (< 1ms) execution overhead. No heavy runtime bloat.
2. Zero/Minimal Dependencies: Prefer standard library and lightweight crates (crossterm, tokio, serde_json).
3. Zero-Trust Security Kernel: Canonicalize paths to guarantee workspace containment. Fail closed on '../' traversal.
4. Terminal Aesthetics: Flush 46-character box headers, continuous dimmed gutter '│ ' streaming, live telemetry line.
5. Surgical Precision: 2-stage AST-friendly replacements. Direct POSIX process group controls with 30s timeouts.

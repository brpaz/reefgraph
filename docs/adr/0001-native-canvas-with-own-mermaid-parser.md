# ADR 0001: Native Canvas with Own Mermaid Parser

## Status
Accepted

## Context
Merman edits Mermaid diagrams visually (drag shapes, drag to connect, inline labels) while keeping the Mermaid source editable next to the canvas. Mermaid's reference implementation is JavaScript and renders SVG in a browser; its layout output is not an editable model and offers no hooks for direct manipulation.

## Decision
- Render diagrams in a custom GTK widget (`MermanCanvas`) using Cairo for geometry and Pango text nodes through `GtkSnapshot`.
- Parse and serialize Mermaid with a hand-written Rust parser per diagram type, starting with the flowchart subset. Unsupported constructs (`subgraph`) are reported as errors; statements the editor does not model (`classDef`, `style`, `click`, comments) round-trip verbatim.
- Keep canvas and text in two-way sync: canvas edits re-serialize the whole diagram into the text buffer; text edits are re-parsed 300 ms after the last keystroke and applied to the canvas. A parse error leaves the canvas untouched and shows a banner.
- Draw all geometry in a single Cairo layer per frame, with text nodes on top. Several Cairo nodes per frame did not render on the Broadway backend.

## Alternatives Considered
- **Embed mermaid.js in WebKitGTK**: exact Mermaid rendering, but no native interaction model, a heavy runtime dependency, and a JS/Rust bridge for every edit.
- **Generate SVG with mermaid-cli and overlay handles**: needs Node.js at runtime and cannot map SVG output back to model elements reliably.
- **GSK paths (`GskPath`) instead of Cairo**: vector output with no rasterization, but requires GTK 4.14+ APIs across all drawing code; Cairo is simpler and adequate for diagram sizes Merman targets.

## Consequences
- Full control over interaction (handles, rubber-band connections, drop targets, undo history).
- Rendering is close to, not identical with, Mermaid's output; the exported `.mmd` renders with Mermaid's own layout elsewhere.
- Each new diagram type (sequence, ERD) needs its own model, parser, serializer and renderer.
- Canvas edits rewrite the source formatting into a canonical form (node definitions, then links, then passthrough statements).

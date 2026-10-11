# ADR 0004: Export Images by Replaying the Canvas Scene

## Status
Accepted

## Context
Diagrams need to be exported as SVG and PNG. The canvases draw geometry with Cairo and labels as Pango layouts appended to a `gtk::Snapshot`, so there is no single drawing backend that could target a file. Merman has its own layout, so a diagram exported by another renderer (such as mermaid-cli) would not look like the canvas and would lose manual positions.

## Decision
- Each canvas implements `export::Exportable`: `content_bounds` and `draw_scene(snapshot, scene)`. Its `snapshot` vfunc calls `draw_scene` with an interactive scene for the widget; export calls it with a non-interactive scene (no grid, selection, handles or drag feedback) in light colors, with the viewport fitted to the content plus a margin.
- Export takes the render node from that snapshot and draws it onto a Cairo `SvgSurface` or `ImageSurface` with `gsk::RenderNode::draw`. GSK records Cairo nodes on recording surfaces, so geometry stays vector in SVG output; text is emitted as glyph paths.
- The format is chosen from the file name extension: `.png` gives PNG, anything else SVG.

## Alternatives Considered
- **Pass a `cairo::Context` to the draw code instead of a snapshot**: needs label drawing rewritten with pangocairo for both paths; more code for the same output.
- **Shell out to mermaid-cli**: adds a Node.js and headless-browser dependency, and the result ignores Merman's layout and stored positions.
- **`gsk::Renderer::render_texture` for PNG**: needs a realized widget and renderer, and still leaves SVG unsolved.

## Consequences
- Any change to canvas rendering shows up in exports with no extra work.
- Exported SVG text is outlines, not selectable `<text>`; files are larger and text cannot be edited in a vector editor.
- Export depends on the Cairo `svg` and `png` features through a direct `cairo-rs` dependency, which must stay on the same version as the one `gtk4` uses.

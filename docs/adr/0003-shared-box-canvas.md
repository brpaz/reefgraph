# ADR 0003: Shared Canvas for Box-and-Link Diagrams

## Status
Accepted

## Context
Class diagrams and entity relationship diagrams are edited the same way: boxes with rows are moved by dragging, linked by dragging from handles, given rows with a "+" button and edited in place, with undo and pan/zoom. The class diagram canvas started as a copy of the ERD canvas, leaving two widgets of about 900 lines that differed only in model type, box geometry, rendering and how edited text applies. Fixes had to be made twice and had started to drift.

## Decision
- One widget, `MermanBoxCanvas`, implements the interaction once and edits any type implementing the `BoxDiagram` trait: box geometry (`boxes`, `box_rect`, `header_center`), labels and row text, adding/removing boxes, links and rows, layout and rendering.
- The canvas stores the diagram as `Box<dyn BoxDiagram>`. Undo history clones it through `clone_diagram`, and change detection compares through `same_as`.
- Inspectors and the window, which know the concrete type, use `BoxCanvas::with::<M>` and `BoxCanvas::edit::<M>`. These downcast through `Any` and do nothing when the canvas holds another type.
- Flowcharts and sequence diagrams keep their own canvases: free-form shapes and lifeline rows interact differently enough that sharing would need more configuration than code it saves.

## Alternatives Considered
- **Keep one canvas per diagram type**: no indirection, but duplicated interaction code for every new box-and-link type.
- **Generic widget `BoxCanvas<M>`**: GObject subclasses cannot be generic, so this would need a macro generating one subclass per type, hiding the code behind macro expansion.
- **Domain trait instead of editor trait**: keeps rendering out of the trait, but splits a diagram type's editor behaviour across two traits without removing any code.

## Consequences
- Interaction fixes and features (handles, the add-row button, rename handling) apply to both diagram types at once.
- A new box-and-link diagram type needs a domain module, a `BoxDiagram` implementation and an inspector, but no canvas.
- Type errors between an inspector and its canvas are no longer caught at compile time; a mismatched `with::<M>`/`edit::<M>` silently does nothing.
- Each hit test measures label text through Pango for every box; fine for diagrams of a few hundred boxes, and the place to add caching if larger ones become slow.

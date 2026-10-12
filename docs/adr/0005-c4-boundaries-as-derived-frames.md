# ADR 0005: C4 Boundaries as Derived Frames

## Status
Accepted

## Context
C4 container and component diagrams group elements in boundaries (`Enterprise_Boundary`, `System_Boundary`, `Container_Boundary`, `Boundary`), which nest. Mermaid places them itself and stores no geometry for them. The box canvas edits boxes and links; it has no notion of containers. Flowchart subgraphs, the same kind of nesting, are still unsupported, so this is the first grouping construct the editor handles.

## Decision
- Boundaries are part of the model only as membership: each element and boundary names the boundary it is declared in. They have no position or size.
- Each boundary is drawn as a frame computed from what it contains: the union of its members' boxes and nested frames, padded, with the label and type below the contents. `BoxDiagram::decorations` reports the frames so zoom-to-fit and export include them; they are not boxes, so they cannot be selected, moved or linked.
- Membership, names and nesting are edited in the inspector. Dragging an element moves it within or out of its frame's drawn area without changing membership.
- Auto layout is compound: a boundary's contents are laid out first, and the boundary then takes part in its parent level's layered layout as one node of that size, with relations lifted to the nodes they connect at that level. Members stay together and other elements stay out of their frames.

## Alternatives Considered
- **Boundaries as boxes with stored positions and sizes**: frames could be dragged and resized, but each needs its own position comment, and keeping members inside a manually sized frame needs constraint handling on every move.
- **Reject boundaries for now**: the quickest path, but most container diagrams use `System_Boundary`, so the editor could not open them.
- **Flat layered layout with frames drawn afterwards**: unrelated elements are placed inside frames whenever ranks interleave.

## Consequences
- Moving an element far from the rest of its boundary stretches the frame over whatever lies between; the editor does not keep other elements out after manual moves.
- Edge routing ignores frames, so a relation can cross a frame or its label.
- The same approach can serve flowchart subgraphs, which are also groups without stored geometry.

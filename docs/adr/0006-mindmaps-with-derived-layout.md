# ADR 0006: Mindmaps with a Derived Layout

## Status
Accepted

## Context
Mermaid mindmaps are an indented outline: a node's parent is the closest less indented line above it. Most nodes are plain text without an id, several nodes may share the same text, and Mermaid lays mindmaps out itself. The other node-link diagrams keep manual positions as `%% @reefgraph pos <id> <x> <y>` comments (ADR 0002), which needs a stable, whitespace-free id per node. Mindmap editing is also mostly structural (adding children, moving a branch) rather than placing shapes.

## Decision
- Mindmaps are edited on the shared box canvas (ADR 0003). Nodes get editor-only keys (`n0`, `n1`…) in source order; keys are never written to the source.
- No positions are stored in the source. The layout is derived from the tree whenever boxes are measured: the root's children alternate to its right and left, and each branch grows outwards with its subtrees stacked. The root's position is the only one kept, in the editor model: it is where the root was created, it carries over when the source is re-parsed, and dragging the root moves the whole mindmap. Dragging any other node does nothing, and the canvas records no undo step for a drag that leaves the diagram unchanged.
- A node's rows are its children: the "add row" button adds a child and edits its text in place, and clearing a child's text removes it.
- Dragging from a node's handle onto another node moves the target, with its subtree, under the source. Moves of the root, into the node's own subtree or under its current parent are refused. Links cannot be deleted on their own, since every node but the root has exactly one parent.
- Indentation is rewritten as two spaces per level; icon, class and comment lines stay attached to the node above them.

## Alternatives Considered
- **Position comments keyed by node text**: texts repeat and contain spaces, so positions would attach to the wrong nodes or need escaping.
- **Generating ids for every node**: the editor would add ids to every plain node, rewriting the source of mindmaps that were only opened.
- **A separate mindmap canvas**: tree-specific gestures (insert sibling, reorder siblings by dragging) would fit better, but the box canvas already provides selection, in-place editing, undo, export and navigation.

## Consequences
- Mindmaps cannot be arranged by hand beyond moving them as a whole; the drawn layout differs from Mermaid's own, which also places nodes itself.
- A reopened mindmap grows from the canvas origin, and the view is fitted to it.
- Each hit test and draw recomputes the layout, which is linear in the number of nodes. Branches are drawn straight from parent to child (`BoxDiagram::routes_around_boxes`), skipping the detour routing that checks every link against every box.
- Sibling order can only be changed in the source, or by moving a node away and back, which appends it as the last child.
- Re-parsing the source renumbers the keys, so a selection may move to another node after the text is edited.

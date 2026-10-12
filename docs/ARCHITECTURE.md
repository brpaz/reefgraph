# Architecture

Reefgraph is a GTK4/libadwaita application written in Rust for editing Mermaid flowcharts, sequence diagrams, state diagrams, use case diagrams, class diagrams, entity relationship diagrams, C4 diagrams and mindmaps. UI layouts are Blueprint files compiled to GtkBuilder XML at build time and bundled into a GResource.

## Modules

```
src/
├── main.rs              # i18n, resource loading, application start
├── application.rs       # AdwApplication: actions, accelerators, about dialog, file open from CLI
├── window.rs            # Main window: picks the editor for the diagram type, source <-> canvas sync, file I/O
├── diagram.rs           # Diagram type detection and parse dispatch; DiagramKind templates and samples
├── new_diagram.rs       # New Diagram dialog: one tile per DiagramKind with a rendered sample preview
├── syntax.rs            # Shared parsing: ParseError, front matter/declaration split, `;` statements
├── ids.rs               # Element id rules shared by the models
├── geometry.rs          # Point, Rect
├── viewport.rs          # Pan/zoom mapping between canvas and widget coordinates
├── history.rs           # Snapshot undo/redo stack
├── layered.rs           # Layered auto layout and flow Direction, for node-link diagrams
├── routing.rs           # Edge paths: outline clipping, parallel bends, detours around nodes
├── drawing.rs           # Colors, label layout, text and Cairo helpers shared by the canvases
├── interaction.rs       # Navigable trait (scroll/zoom/pan), InlineEditor popover
├── export.rs            # SVG/PNG export: Exportable trait, Scene, render to a Cairo surface
├── markdown.rs          # Mermaid fenced blocks in Markdown: find, extract, write back
├── box_canvas.rs        # ReefgraphBoxCanvas: shared canvas for box-and-link diagrams (BoxDiagram trait)
├── palette.rs           # ReefgraphPalette: tiles supplied by the active editor
├── combo.rs, labels.rs  # Combo row models and translatable enum names
├── flowchart/           # Flowchart domain (no GTK): model, parser, serializer, layered layout
├── flowchart_editor/    # ReefgraphFlowchartCanvas, ReefgraphFlowchartInspector, shape geometry and rendering
├── sequence/            # Sequence domain (no GTK): model, parser, serializer, layout and hit testing
├── sequence_editor/     # ReefgraphSequenceCanvas, ReefgraphSequenceInspector, rendering
├── state/               # State diagram domain (no GTK): model, parser, serializer, state geometry, layout
├── state_editor/        # BoxDiagram for state diagrams, ReefgraphStateInspector, state shapes
├── usecase/             # Use case domain (no GTK): model, parser, serializer, element geometry, boundary frames, compound layout
├── usecase_editor/      # BoxDiagram for use case diagrams, ReefgraphUseCaseInspector, actor and relationship drawing
├── mindmap/             # Mindmap domain (no GTK): tree model, indentation parser, serializer, two-sided tree layout
├── mindmap_editor/      # BoxDiagram for mindmaps, ReefgraphMindmapInspector, node shapes and branches
├── class_diagram/       # Class diagram domain (no GTK): model, parser, serializer, class box geometry, layout
├── class_editor/        # BoxDiagram for class diagrams, ReefgraphClassInspector, UML relation heads
├── erd/                 # ERD domain (no GTK): model, parser, serializer, entity box geometry, layout
├── erd_editor/          # BoxDiagram for ERDs, ReefgraphErdInspector, crow's-foot rendering
├── c4/                  # C4 domain (no GTK): model, parser, serializer, element boxes, boundary frames, compound layout
└── c4_editor/           # BoxDiagram for C4 diagrams, ReefgraphC4Inspector, element shapes and frames

data/resources/ui/*.blp  # Blueprint templates for window, palette, inspectors, shortcuts
```

New documents are created from the **New Diagram** dialog (Ctrl+N or the "+" header button). Its **From Clipboard** section reads the clipboard text when the dialog opens and offers a tile for every diagram in it that parses: the whole text, or each `mermaid` block when it is Markdown (`markdown::pasted_diagrams`). Invalid text yields no tile, so the current document is never replaced by a broken one. Below it, **Empty Diagram** lists `DiagramKind::ALL`; each tile shows the kind's sample diagram rendered with the export code, a name and a description from `labels.rs`. The dialog hands the window the chosen source text, which it loads as an untitled document. Adding a diagram type to the dialog takes a `DiagramKind` variant with a template and sample, its labels, and an arm in `Diagram::for_canvas`, which routes a parsed diagram to the flowchart, sequence or box canvas for previews and editing.

Each diagram type is a pair of modules: a GTK-free domain module covered by plain `cargo test`, and an editor module with its properties panel. Editor modules depend on the domain and the shared modules, never on each other.

Flowcharts and sequence diagrams have their own canvas widgets. State, use case, class, entity relationship and C4 diagrams and mindmaps share `ReefgraphBoxCanvas`, which implements moving, linking, the add-row button, in-place editing, undo and navigation once; each diagram type implements the `BoxDiagram` trait (geometry, rendering, what rows are and how edited text applies). Inspectors reach the typed model through `BoxCanvas::with::<M>` and `BoxCanvas::edit::<M>`; see [ADR 0003](adr/0003-shared-box-canvas.md).

## Data Flow

```mermaid
flowchart LR
    Text[Source view] -- "debounced parse" --> Window
    Window -- "set_chart / set_sequence / set_diagram" --> Canvas
    Canvas -- "changed: serialize" --> Text
    Palette -- "click / drag" --> Canvas
    Inspector -- "edit()" --> Canvas
    Canvas -- "selection-changed" --> Inspector
```

- The window shows one editor at a time, chosen from the diagram declaration (`flowchart`/`graph`, `sequenceDiagram`, `stateDiagram`/`stateDiagram-v2`, `usecase-beta`, `classDiagram`, `erDiagram`, a C4 keyword or `mindmap`). Typing a different declaration in the source loads the diagram into the other editor.
- Each canvas owns its model. Every user edit goes through `edit`/`try_edit`, which records an undo snapshot and emits `changed`.
- The window turns `changed` into Mermaid text, replacing the buffer contents as one undoable step. While it does so it sets a `syncing` flag so the change is not parsed back.
- Text edits are parsed 300 ms after the last keystroke. A valid result replaces the canvas model; a parse error is shown in a banner and the canvas is left alone.
- Canvas signals: `changed` (user edit), `selection-changed`, `history-changed` (undo/redo availability).

## Canvas Interaction

All canvases: scroll to pan, Ctrl+scroll to zoom around the pointer, middle-drag or drag on empty space to pan, double-click to edit text in place, Delete, F2, Ctrl+Z, Ctrl+Shift+Z, Escape.

Ctrl+C copies the selected node, box, participant, message or note into the canvas; Ctrl+V adds a copy as it was when copied. Nodes and boxes land diagonally next to where they were copied from, participants right of the original, messages and notes below the selected row (or the original row). Copies take a fresh id (`ids::next_free_id` for nodes and participants, `ids::copy_id` for classes and entities) and keep a custom label. The clipboard is per canvas, not the system clipboard, so the text editor's copy and paste are unaffected.

### Flowchart

| Gesture | Effect |
|---|---|
| Drag node | Move (snapped to a 10 px grid) |
| Drag from a node handle, or Shift+drag a node | Create a link to the node under the pointer on release |
| Double-click empty space | Add a rectangle and edit its label |
| Drop a palette tile | Add that shape at the drop point |

Edges between the same pair of nodes are bent apart. An edge whose straight path crosses another node is rerouted as an arc on the first side that clears all nodes.

### Sequence diagram

| Gesture | Effect |
|---|---|
| Drag a participant box sideways | Reorder participants |
| Drag from a lifeline to another lifeline | Add a message at that height (right of its own lifeline: message to self) |
| Drag a message, note or block edge up or down | Move that row; moves that would unbalance a block are rejected |
| Double-click the header area | Add a participant |
| Drop a palette tile | Participant/actor: insert at that column. Note: right of the nearest lifeline. Block: wrap the message or note under the pointer, or add an empty block |

### Entity relationship diagram

| Gesture | Effect |
|---|---|
| Drag entity | Move (snapped to a 10 px grid) |
| Drag from an entity handle, or Shift+drag an entity | Create a one-to-many relationship to the entity under the pointer |
| Click the "+" below the selected entity | Add an attribute and edit it |
| Double-click the header | Rename the entity, or edit its alias when it has one |
| Double-click an attribute | Edit the attribute as text: `type name [PK, FK, UK] ["comment"]` |
| Double-click empty space | Add an entity and name it |

Relationships use the shared edge routing and are drawn with crow's-foot markers for their cardinalities; non-identifying ones are dashed.

### State diagram

Same gestures as entity relationship diagrams, without the "+" button: drag to move, drag from a handle to add a transition, double-click a state to rename it or edit its description, and a transition to edit its label. The palette adds states, the start and end states (`[*]`), and `<<choice>>`, `<<fork>>` and `<<join>>` pseudo-states; there is one start and one end state. Transitions into the start state or out of the end state are refused, since `[*]` could not express them. Links end on the outline of each shape (`BoxDiagram::outline`), so arrows meet circles and diamonds rather than their bounding boxes.

### Use case diagram

Same gestures as C4 diagrams: drag to move, drag from a handle to add a `-->` association, double-click an element to rename it or edit its label, and an association to edit its label. Relationship types (the seven associations, include, extend and generalization) are set in the inspector. System boundaries are listed on the diagram page and drawn as frames around their members, like C4 boundaries (ADR 0005); auto layout lays each boundary's members out first and then places the boundary as one node.

### Mindmap

Mindmaps have no stored positions: the root's children alternate to its right and left and each branch grows outwards, recomputed from the tree on every draw around the root's position (ADR 0006). Dragging the root moves the whole mindmap; dragging other nodes does nothing. The "+" under the selected node adds a child and edits its text; dragging from a node's handle onto another node moves the target and its subtree under the source. The palette and double-clicking empty space add a child of the root, or the root of an empty mindmap. Deleting a node deletes its subtree; the root can only be deleted when it is the last node. Each branch takes its own color.

### Class diagram

Same gestures as entity relationship diagrams: drag to move, drag from a handle to relate (a `-->` association), "+" under the selected class adds a member, double-click the header to rename and a member to edit its text. Members are kept as written; those containing `(` are listed in the method compartment, `$` and `*` render underlined (static) and italic (abstract), and `~T~` generics display as `<T>`. Relation heads (inheritance, composition, aggregation, arrow, lollipop) are set per end in the inspector.

### C4 diagram

Same gestures as entity relationship diagrams, without the "+" button: drag to move, drag from a handle to add a `Rel` labelled "Uses", double-click the name to rename, the technology line or the area below the name to edit the technology or description. Kind, shape (box, database, queue), external, boundary membership and relation details are set in the inspector, which also lists the boundaries.

Boundaries are not boxes: they have no position of their own. Each is drawn as a frame around the elements and nested boundaries declared in it (`BoxDiagram::decorations`), with its label at the bottom left, and moves and grows with them. Auto layout is compound: every boundary's contents are laid out first, then the boundary takes part in its parent level as one node, with relations lifted to the nodes they connect at that level. See [ADR 0005](adr/0005-c4-boundaries-as-derived-frames.md).

### Blocks in sequence diagrams

Blocks (`loop`, `alt`, `opt`, `par`, `critical`, `break`, `rect`) are stored as flat start/branch/end markers in the item list, so their extent changes by dragging the markers like any other row. `Sequence::validate` keeps them balanced.

## Image Export

**Export Image** (Ctrl+E) writes the current diagram as SVG, or as PNG when the chosen file name ends in `.png`. Each canvas implements `export::Exportable`: `content_bounds` (shapes plus edge routes, also used by zoom-to-fit) and `draw_scene`, which its `snapshot` calls too. A `Scene` carries the viewport, size, colors and whether to draw the grid, selection and drag feedback. Export draws a non-interactive scene in light colors on white into a `gtk::Snapshot` and replays the resulting render node onto a Cairo SVG or image surface, so on-screen and exported drawing cannot diverge; see [ADR 0004](adr/0004-export-by-replaying-the-canvas-scene.md). PNGs are rendered at 2× scale.

## File Format

Files are plain Mermaid, or Markdown documents (`.md`, `.markdown`) with a diagram in a ```` ```mermaid ```` or `~~~mermaid` fenced block. Opening a Markdown file edits one block, chosen from a list when there are several; saving writes the document back with only that block's contents replaced. The fence's indentation is removed for editing and restored on save, so blocks nested in list items round-trip. Unclosed fences are ignored. When a line of the diagram would close the block early, both fences are lengthened; lines are written with the document's line ending. **Save As** to a Markdown file writes the whole document; to any other file it writes the diagram alone, and the document is no longer linked. **Copy Mermaid Source** (Ctrl+Shift+C) copies the source without the `%% @reefgraph pos <id> <x> <y>` lines (other comments and line endings are kept), for pasting where Mermaid is rendered.

- Flowcharts: manually placed nodes are stored as `%% @reefgraph pos <id> <x> <y>` comments (`%% @merman pos`, written before the app was renamed, is still read); see [ADR 0002](adr/0002-store-node-positions-in-mermaid-comments.md). `classDef`, `class`, `style`, `linkStyle`, `click` and comments are kept verbatim. `subgraph` is rejected with a parse error.
- Entity relationship diagrams: manually placed entities use the same position comments as flowcharts. Relationships written with word aliases (`only one to zero or more`) are normalized to symbols when the canvas rewrites the source. `classDef`, `class`, `style` and comments are kept verbatim.
- Class diagrams: manually placed classes use the same position comments. Classes only mentioned in relations are declared with `class Name` when the canvas rewrites the source. `note`, `click`, `link`, `callback`, `cssClass`, `classDef`, `style` and comments are kept verbatim; `namespace` and `:::style` are rejected with a parse error.
- C4 diagrams: `C4Context`, `C4Container` and `C4Component` are supported; `C4Dynamic` and `C4Deployment` are rejected with a parse error. Manually placed elements use the same position comments. Arguments after the description (sprite, tags, link, `$name=` options) are kept as written; `Rel_Back` is rewritten as `Rel` with swapped ends and `Rel_Up`/`Rel_Down`/`Rel_Left`/`Rel_Right` as their short forms. At each nesting level elements are written before boundaries. `Update…Style`, `UpdateLayoutConfig`, `Add…Tag` and comments are kept verbatim.
- State diagrams: `stateDiagram` and `stateDiagram-v2` are supported. `[*]` is stored as a start state (`[*]start`) when it is a transition's source and an end state (`[*]end`) when it is the target, so positions are kept as `%% @reefgraph pos [*]start <x> <y>`; a positioned start or end state exists even before a transition reaches it. Descriptions are written `id : text`, one statement per line, including those declared as `state "text" as id`. Composite states, concurrent regions (`--`) and `:::style` are rejected with a parse error. Notes (single and multi-line), `classDef`, `class`, `style`, `hide`, `scale`, `accTitle`, `accDescr` and comments are kept verbatim.
- Use case diagrams (`usecase-beta`, a beta diagram of Mermaid 12): actors, ellipse and rectangle use cases, all relationship operators with labels, extra dashes and edge ids, and one level of `systemBoundary` blocks. Labels, metadata (`@{ … }`), stereotypes and `:::` classes are kept as written; labels edited in the editor are written quoted. Endpoints only used in relationships are declared on the way out. Notes, `classDef`, `class`, `style`, `accTitle`, `accDescr` and standalone `id@{ … }` statements are kept verbatim. JSON tables and statements spanning several lines (Markdown labels with line breaks, multi-line metadata) are rejected with a parse error.
- Mindmaps: every shape is supported. Node text is kept as written; text typed in the editor that contains delimiters is quoted, and line breaks become `<br/>`. Indentation is rewritten as two spaces per level; when reading, a tab counts as four columns. `::icon(…)`, `:::class` and comment lines stay attached to the node above them. Node text spanning several lines is rejected with a parse error.
- Sequence diagrams: layout follows participant and statement order, so no positions are stored. Participants are always declared explicitly when serializing. `links`, `link`, `properties`, `details` and comments are kept verbatim. `box`, `create` and `destroy` are rejected with a parse error.

## Decisions

- [ADR 0001: Native canvas with own Mermaid parser](adr/0001-native-canvas-with-own-mermaid-parser.md)
- [ADR 0002: Store node positions in Mermaid comments](adr/0002-store-node-positions-in-mermaid-comments.md)
- [ADR 0003: Shared canvas for box-and-link diagrams](adr/0003-shared-box-canvas.md)
- [ADR 0004: Export images by replaying the canvas scene](adr/0004-export-by-replaying-the-canvas-scene.md)
- [ADR 0005: C4 boundaries as derived frames](adr/0005-c4-boundaries-as-derived-frames.md)
- [ADR 0006: Mindmaps with a derived layout](adr/0006-mindmaps-with-derived-layout.md)

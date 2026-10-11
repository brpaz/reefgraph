# ADR 0002: Store Node Positions in Mermaid Comments

## Status
Accepted

## Context
Mermaid has no syntax for node coordinates; its renderer always lays diagrams out automatically. Users of a visual editor expect nodes they move to stay where they put them, and the saved file must stay valid Mermaid so it renders on GitHub, in docs sites and in other tools.

## Decision
- The file format is plain Mermaid source (`.mmd`).
- Manually placed nodes are persisted as comment lines at the end of the file: `%% @merman pos <id> <x> <y>`. Mermaid ignores `%%` comments.
- Nodes without a stored position are placed by a layered auto layout (longest-path ranking, barycenter ordering, cycle-breaking DFS), offset to stay consistent with already-placed nodes.
- While editing the text, nodes keep their current canvas position even when the text has no position line for them, so typing does not make nodes jump. "Auto Layout" discards all stored positions.

## Alternatives Considered
- **Sidecar file** (`diagram.mmd.merman.json`): keeps the Mermaid clean, but files get separated when copied or committed alone.
- **Auto layout only**: no extra metadata, but manual arrangement is lost on every edit, defeating a WYSIWYG editor.
- **YAML front matter**: valid Mermaid, but front matter is reserved for Mermaid config and is rendered as diagram title by some tools.

## Consequences
- Files open in any Mermaid renderer; the stored positions only affect Merman.
- Other renderers still lay the diagram out their own way, so Merman's arrangement is not portable.
- Position lines add noise to diffs when nodes are moved.
- Renaming a node in a text editor outside Merman orphans its position line; it is ignored on load.

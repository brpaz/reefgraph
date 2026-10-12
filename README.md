# Reefgraph

> Visual WYSIWYG editor for Mermaid diagrams for Linux. Built with GTK, Rust and LibAdwaita.

[![CI](https://img.shields.io/github/actions/workflow/status/brpaz/reefgraph/ci.yml?branch=main&label=CI&logo=github&style=for-the-badge)](https://github.com/brpaz/reefgraph/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue?style=for-the-badge)](LICENSE.md)
[![Flatpak](https://img.shields.io/badge/flatpak-ready-4A90D9?style=for-the-badge&logo=flatpak&logoColor=white)](https://flatpak.org)

<div align="center">

![Main window](data/resources/screenshots/screenshot1.png "Main window")

</div>

## AI Usage notice

This project was fully vibecoded in a Weekend using Claude Opus 5.5 to solve a personal itch. Use it at your own risk.

## Features

- Visual editing of Mermaid flowcharts: drag shapes from the palette, drag between nodes to link them, double-click to edit labels
- Visual editing of sequence diagrams: participants and actors, messages drawn by dragging between lifelines, notes, activations, autonumbering and `loop`/`alt`/`opt`/`par`/`critical`/`break`/`rect` blocks; reorder participants and rows by dragging
- Visual editing of state diagrams: states with descriptions, start and end states, choice, fork and join pseudo-states, and labelled transitions drawn by dragging between states
- Visual editing of use case diagrams: actors, use cases, system boundaries drawn as frames, associations, include, extend and generalization
- Visual editing of mindmaps: add children with one click, move branches by dragging between nodes, every Mermaid node shape, automatic two-sided layout
- Visual editing of class diagrams: classes with stereotypes, generics and attribute/method compartments, UML relations (inheritance, composition, aggregation, association, dependency, realization, lollipop) with cardinalities
- Visual editing of entity relationship diagrams: entities drawn as tables with typed attributes and PK/FK/UK keys, relationships with crow's-foot cardinalities created by dragging between entities
- Visual editing of C4 context, container and component diagrams: people, systems, containers and components (also as databases, queues and external elements), relationships with technology, and nested boundaries drawn as frames around their members
- Live two-way sync with the Mermaid source shown next to the canvas
- All flowchart node shapes, solid/dotted/thick links, arrow/circle/cross heads, link labels
- Automatic layered layout; manual positions are saved as Mermaid comments, so files stay valid Mermaid
- Start a diagram from the clipboard: the New Diagram dialog previews every Mermaid diagram copied as plain text or inside Markdown
- Edit diagrams embedded in Markdown files: open a `.md` file, pick one of its `mermaid` code blocks and save it back in place
- Copy the Mermaid source without editor metadata, ready to paste into Markdown
- Export diagrams as SVG or PNG images
- Undo/redo, zoom and pan, keyboard shortcuts

## Install

Download the package for your system from the [latest release](https://github.com/brpaz/reefgraph/releases/latest):

```bash
# Flatpak
flatpak install --user dev.brunopaz.Reefgraph.flatpak
flatpak run dev.brunopaz.Reefgraph

# Debian/Ubuntu
sudo apt install ./reefgraph.deb

# Fedora
sudo dnf install ./reefgraph.rpm
```

## Build from Source

```bash
git clone https://github.com/brpaz/reefgraph
cd reefgraph
task run
```

Full prerequisites, running tests, building the Flatpak/`.deb`/`.rpm`, and the Taskfile reference: [CONTRIBUTING.md](CONTRIBUTING.md).

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md).

## Contributors

<a href="https://github.com/brpaz/reefgraph/graphs/contributors">
  <img src="https://contrib.rocks/image?repo=brpaz/reefgraph" alt="Contributors" />
</a>

## License

This repo is MIT licensed. See [LICENSE.md](LICENSE.md).

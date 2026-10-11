# Merman

> Visual WYSIWYG editor for Mermaid diagrams

[![CI](https://img.shields.io/github/actions/workflow/status/brpaz/merman/ci.yml?branch=main&label=CI&logo=github&style=for-the-badge)](https://github.com/brpaz/merman/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue?style=for-the-badge)](LICENSE.md)
[![Flatpak](https://img.shields.io/badge/flatpak-ready-4A90D9?style=for-the-badge&logo=flatpak&logoColor=white)](https://flatpak.org)

<div align="center">

![Main window](data/resources/screenshots/screenshot1.png "Main window")

</div>

## Features

- Visual editing of Mermaid flowcharts: drag shapes from the palette, drag between nodes to link them, double-click to edit labels
- Visual editing of sequence diagrams: participants and actors, messages drawn by dragging between lifelines, notes, activations, autonumbering and `loop`/`alt`/`opt`/`par`/`critical`/`break`/`rect` blocks; reorder participants and rows by dragging
- Visual editing of class diagrams: classes with stereotypes, generics and attribute/method compartments, UML relations (inheritance, composition, aggregation, association, dependency, realization, lollipop) with cardinalities
- Visual editing of entity relationship diagrams: entities drawn as tables with typed attributes and PK/FK/UK keys, relationships with crow's-foot cardinalities created by dragging between entities
- Live two-way sync with the Mermaid source shown next to the canvas
- All flowchart node shapes, solid/dotted/thick links, arrow/circle/cross heads, link labels
- Automatic layered layout; manual positions are saved as Mermaid comments, so files stay valid Mermaid
- Copy the Mermaid source without editor metadata, ready to paste into Markdown
- Export diagrams as SVG or PNG images
- Undo/redo, zoom and pan, keyboard shortcuts

## Install

Download the package for your system from the [latest release](https://github.com/brpaz/merman/releases/latest):

```bash
# Flatpak
flatpak install --user dev.brunopaz.Merman.flatpak
flatpak run dev.brunopaz.Merman

# Debian/Ubuntu
sudo apt install ./merman.deb

# Fedora
sudo dnf install ./merman.rpm
```

## Build from Source

```bash
git clone https://github.com/brpaz/merman
cd merman
task run
```

Full prerequisites, running tests, building the Flatpak/`.deb`/`.rpm`, and the Taskfile reference: [CONTRIBUTING.md](CONTRIBUTING.md).

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md).

## Contributors

<a href="https://github.com/brpaz/merman/graphs/contributors">
  <img src="https://contrib.rocks/image?repo=brpaz/merman" alt="Contributors" />
</a>

## License

This repo is MIT licensed. See [LICENSE.md](LICENSE.md).

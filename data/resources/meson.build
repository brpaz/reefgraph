blueprints = custom_target(
  'blueprints',
  input: files(
    'ui/class-inspector.blp',
    'ui/erd-inspector.blp',
    'ui/flowchart-inspector.blp',
    'ui/palette.blp',
    'ui/sequence-inspector.blp',
    'ui/shortcuts.blp',
    'ui/window.blp',
  ),
  output: '.',
  command: [
    find_program('blueprint-compiler'),
    'batch-compile',
    '@OUTPUT@',
    '@CURRENT_SOURCE_DIR@',
    '@INPUT@',
  ],
)

resources = gnome.compile_resources(
  'resources',
  'resources.gresource.xml',
  gresource_bundle: true,
  source_dir: meson.current_build_dir(),
  dependencies: blueprints,
  install: true,
  install_dir: pkgdatadir,
)

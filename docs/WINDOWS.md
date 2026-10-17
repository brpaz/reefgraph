# Adding a New Application Window

This guide explains how to add additional windows to your GTK application beyond the main window.

## Overview

In this template, windows are implemented as GObject subclasses with a corresponding GTK UI template file. The main window follows a specific pattern that you can replicate for additional windows.

## Step 1: Create the Window Blueprint

UI is written in [Blueprint](https://gnome.pages.gitlab.gnome.org/blueprint-compiler/) and compiled to GtkBuilder XML at build time. Create `data/resources/ui/settings.blp`:

```blueprint
using Gtk 4.0;
using Adw 1;

template $ReefgraphSettingsWindow: Window {
  title: _("Settings");

  child: Adw.ToolbarView {
    [top]
    Adw.HeaderBar {}

    content: Label headerbar_label {
      label: _("Settings content here");
    };
  };
}
```

Widgets that Rust needs to reach get an id (`Label headerbar_label`), which maps to a `#[template_child]` field of the same name.

Register the file in two places:

`data/resources/meson.build`, in the `blueprints` inputs:

```meson
'ui/settings.blp',
```

`data/resources/resources.gresource.xml`, using the compiled `.ui` name:

```xml
<file compressed="true" preprocess="xml-stripblanks">ui/settings.ui</file>
```

## Step 2: Create the Window Rust Module

Create `src/app/settings.rs` (or add to an existing windows module):

```rust
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gio, glib};

mod imp {
    use super::*;

    #[derive(Debug, gtk::CompositeTemplate)]
    #[template(resource = "/dev/brunopaz/Reefgraph/ui/settings.ui")]
    pub struct ReefgraphSettingsWindow {
        #[template_child]
        pub headerbar_label: TemplateChild<gtk::Label>,
    }

    impl Default for ReefgraphSettingsWindow {
        fn default() -> Self {
            Self {
                headerbar_label: TemplateChild::default(),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ReefgraphSettingsWindow {
        const NAME: &'static str = "ReefgraphSettingsWindow";
        type Type = super::ReefgraphSettingsWindow;
        type ParentType = gtk::Window;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for ReefgraphSettingsWindow {
        fn constructed(&self) {
            self.parent_constructed();
        }
    }

    impl WidgetImpl for ReefgraphSettingsWindow {}
    impl WindowImpl for ReefgraphSettingsWindow {}
}

glib::wrapper! {
    pub struct ReefgraphSettingsWindow(ObjectSubclass<imp::ReefgraphSettingsWindow>)
        @extends gtk::Widget, gtk::Window,
        @implements gio::ActionMap, gio::ActionGroup;
}

impl ReefgraphSettingsWindow {
    pub fn new(app: &ReefgraphApplication) -> Self {
        glib::Object::builder()
            .property("application", app)
            .property("title", "Settings")
            .build()
    }
}
```

## Step 3: Register the Module

Add the module to your crate's `lib.rs`:

```rust
mod window;
pub use window::ReefgraphSettingsWindow;
```

## Step 4: Add a Action to Open the Window

In `src/app/application.rs`, add a new action in `setup_gactions()`:

```rust
fn setup_gactions(&self) {
    // ... existing actions ...
    
    let action_settings = gio::ActionEntry::builder("settings")
        .activate(|app: &Self, _, _| {
            let window = ReefgraphSettingsWindow::new(app);
            window.set_transient_for(Some(&app.main_window()));
            window.present();
        })
        .build();
    
    self.add_action_entries([action_settings]);
}
```

Then add the accelerator in `setup_accels()`:

```rust
fn setup_accels(&self) {
    // ... existing accels ...
    self.set_accels_for_action("app.settings", &["<Control>,"]);
}
```

## Step 5: Add Menu Item (Optional)

Add a menu item in the relevant `.blp` file to trigger the action:

```blueprint
menu primary_menu {
  item {
    label: _("_Settings");
    action: "app.settings";
  }
}
```

## Key Concepts

### Template Child

The `#[template_child]` attribute gives you direct access to child widgets defined in your Blueprint file. The field name must match the widget id:

```rust
#[template_child]
pub headerbar_label: TemplateChild<gtk::Label>,
```

Access it via `self.imp().headerbar_label.get()`.

### Window Parent Types

- `gtk::Window` - Basic window
- `adw::ApplicationWindow` - Window tied to an application (used by the main window)
- `adw::Dialog` - Modal dialog presented over a parent window
- `gtk::Popover` - Popover, not a window

### Modal Windows

Set a window as modal to block interaction with parent:

```rust
window.set_modal(true);
window.set_transient_for(Some(&parent_window));
```

## Complete Example

See the main window implementation in `src/app/window.rs` for a complete reference including:
- GSettings integration for persisting window size
- CSS class handling for devel profiles
- Window state persistence
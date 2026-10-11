# Adding a New Application Window

This guide explains how to add additional windows to your GTK application beyond the main window.

## Overview

In this template, windows are implemented as GObject subclasses with a corresponding GTK UI template file. The main window follows a specific pattern that you can replicate for additional windows.

## Step 1: Create the Window UI Definition

Create a new UI file in `data/resources/ui/`:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<interface>
  <template class="MyAppSettingsWindow" parent="GtkWindow">
    <child>
      <object class="GtkBox">
        <property name="orientation">vertical</property>
        <child>
          <object class="GtkHeaderBar">
            <property name="title">Settings</property>
            <property name="show-close-button">True</property>
          </object>
        </child>
        <child>
          <object class="GtkLabel">
            <property name="label">Settings content here</property>
          </object>
        </child>
      </object>
    </child>
  </template>
</interface>
```

Save this as `data/resources/ui/settings_window.ui`.

## Step 2: Create the Window Rust Module

Create `src/window/settings.rs` (or add to an existing windows module):

```rust
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gio, glib};

mod imp {
    use super::*;

    #[derive(Debug, gtk::CompositeTemplate)]
    #[template(resource = "/com/example/myapp/ui/settings_window.ui")]
    pub struct MyAppSettingsWindow {
        #[template_child]
        pub headerbar: TemplateChild<gtk::HeaderBar>,
    }

    impl Default for MyAppSettingsWindow {
        fn default() -> Self {
            Self {
                headerbar: TemplateChild::default(),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for MyAppSettingsWindow {
        const NAME: &'static str = "MyAppSettingsWindow";
        type Type = super::MyAppSettingsWindow;
        type ParentType = gtk::Window;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for MyAppSettingsWindow {
        fn constructed(&self) {
            self.parent_constructed();
        }
    }

    impl WidgetImpl for MyAppSettingsWindow {}
    impl WindowImpl for MyAppSettingsWindow {}
}

glib::wrapper! {
    pub struct MyAppSettingsWindow(ObjectSubclass<imp::MyAppSettingsWindow>)
        @extends gtk::Widget, gtk::Window,
        @implements gio::ActionMap, gio::ActionGroup;
}

impl MyAppSettingsWindow {
    pub fn new(app: &MyAppApplication) -> Self {
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
pub use window::MyAppSettingsWindow;
```

## Step 4: Add a Action to Open the Window

In `src/application.rs`, add a new action in `setup_gactions()`:

```rust
fn setup_gactions(&self) {
    // ... existing actions ...
    
    let action_settings = gio::ActionEntry::builder("settings")
        .activate(|app: &Self, _, _| {
            let window = MyAppSettingsWindow::new(app);
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

Add a menu item in your UI file to trigger the action:

```xml
<item>
  <attribute name="label" translatable="yes">_Settings</attribute>
  <attribute name="action">app.settings</attribute>
</item>
```

## Key Concepts

### Template Child

The `#[template_child]` attribute gives you direct access to child widgets defined in your UI file:

```rust
#[template_child]
pub headerbar: TemplateChild<gtk::HeaderBar>,
```

Access it via `self.imp().headerbar.get()`.

### Window Parent Types

- `gtk::Window` - Basic window
- `gtk::ApplicationWindow` - Window tied to an application (handles app menu, etc.)
- `gtk::Dialog` - Modal dialog window
- `gtk::Popover` - Popover window

### Modal Windows

Set a window as modal to block interaction with parent:

```rust
window.set_modal(true);
window.set_transient_for(Some(&parent_window));
```

## Complete Example

See the main window implementation in `src/window.rs` for a complete reference including:
- GSettings integration for persisting window size
- CSS class handling for devel profiles
- Window state persistence
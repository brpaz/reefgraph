use gettextrs::gettext;
use tracing::{debug, info};

use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gdk, gio, glib};

use crate::config::{APP_ID, PKGDATADIR, PROFILE, VERSION};
use crate::window::MermanApplicationWindow;

mod imp {
    use super::*;
    use glib::WeakRef;
    use std::cell::OnceCell;

    #[derive(Debug, Default)]
    pub struct MermanApplication {
        pub window: OnceCell<WeakRef<MermanApplicationWindow>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for MermanApplication {
        const NAME: &'static str = "MermanApplication";
        type Type = super::MermanApplication;
        type ParentType = gtk::Application;
    }

    impl ObjectImpl for MermanApplication {}

    impl ApplicationImpl for MermanApplication {
        fn activate(&self) {
            debug!("GtkApplication<MermanApplication>::activate");
            self.parent_activate();
            let app = self.obj();

            if let Some(window) = self.window.get() {
                let window = window.upgrade().unwrap();
                window.present();
                return;
            }

            let window = MermanApplicationWindow::new(&app);
            self.window
                .set(window.downgrade())
                .expect("Window already set.");

            app.main_window().present();
        }

        fn startup(&self) {
            debug!("GtkApplication<MermanApplication>::startup");
            self.parent_startup();
            let app = self.obj();

            // Set icons for shell
            gtk::Window::set_default_icon_name(*APP_ID);

            app.setup_css();
            app.setup_gactions();
            app.setup_accels();
        }
    }

    impl GtkApplicationImpl for MermanApplication {}
}

glib::wrapper! {
    pub struct MermanApplication(ObjectSubclass<imp::MermanApplication>)
        @extends gio::Application, gtk::Application,
        @implements gio::ActionMap, gio::ActionGroup;
}

impl MermanApplication {
    fn main_window(&self) -> MermanApplicationWindow {
        self.imp().window.get().unwrap().upgrade().unwrap()
    }

    fn setup_gactions(&self) {
        // Quit
        let action_quit = gio::ActionEntry::builder("quit")
            .activate(move |app: &Self, _, _| {
                // This is needed to trigger the delete event and saving the window state
                app.main_window().close();
                app.quit();
            })
            .build();

        // About
        let action_about = gio::ActionEntry::builder("about")
            .activate(|app: &Self, _, _| {
                app.show_about_dialog();
            })
            .build();
        self.add_action_entries([action_quit, action_about]);
    }

    // Sets up keyboard shortcuts
    fn setup_accels(&self) {
        self.set_accels_for_action("app.quit", &["<Control>q"]);
        self.set_accels_for_action("window.close", &["<Control>w"]);
    }

    fn setup_css(&self) {
        let provider = gtk::CssProvider::new();
        provider.load_from_resource("/dev/brunopaz/Merman/style.css");
        if let Some(display) = gdk::Display::default() {
            gtk::style_context_add_provider_for_display(
                &display,
                &provider,
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
        }
    }

    fn authors() -> Vec<&'static str> {
        // Authors are defined in Cargo.toml
        env!("CARGO_PKG_AUTHORS").split(":").collect()
    }

    fn show_about_dialog(&self) {
        let dialog = gtk::AboutDialog::builder()
            .logo_icon_name(*APP_ID)
            .website("https://github.com/brpaz/merman")
            .version(*VERSION)
            .transient_for(&self.main_window())
            .translator_credits(gettext("translator-credits"))
            .modal(true)
            .authors(Self::authors())
            .artists(vec!["Merman contributors"])
            .build();

        dialog.present();
    }

    pub fn run(&self) -> glib::ExitCode {
        info!("{} ({})", "Merman", *APP_ID);
        info!("Version: {} ({})", *VERSION, *PROFILE);
        info!("Datadir: {}", *PKGDATADIR);

        ApplicationExtManual::run(self)
    }
}

impl Default for MermanApplication {
    fn default() -> Self {
        glib::Object::builder()
            .property("application-id", *APP_ID)
            .property("resource-base-path", "/dev/brunopaz/Merman/")
            .build()
    }
}

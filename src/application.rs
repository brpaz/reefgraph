use gettextrs::gettext;
use tracing::{debug, info};

use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::{gio, glib};

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
        type ParentType = adw::Application;
    }

    impl ObjectImpl for MermanApplication {}

    impl ApplicationImpl for MermanApplication {
        fn activate(&self) {
            debug!("GtkApplication<MermanApplication>::activate");
            self.parent_activate();
            let app = self.obj();

            if let Some(window) = self.window.get().and_then(|w| w.upgrade()) {
                window.present();
                return;
            }

            let window = MermanApplicationWindow::new(&app);
            self.window
                .set(window.downgrade())
                .expect("Window already set.");

            app.main_window().present();
        }

        fn open(&self, files: &[gio::File], _hint: &str) {
            self.activate();
            let window = self.obj().main_window();
            if let Some(file) = files.first().cloned() {
                glib::spawn_future_local(async move { window.load_file(&file).await });
            }
        }

        fn startup(&self) {
            debug!("GtkApplication<MermanApplication>::startup");
            self.parent_startup();
            let app = self.obj();

            sourceview5::init();
            gtk::Window::set_default_icon_name(*APP_ID);

            app.setup_gactions();
            app.setup_accels();
        }
    }

    impl GtkApplicationImpl for MermanApplication {}
    impl AdwApplicationImpl for MermanApplication {}
}

glib::wrapper! {
    pub struct MermanApplication(ObjectSubclass<imp::MermanApplication>)
        @extends gio::Application, gtk::Application, adw::Application,
        @implements gio::ActionMap, gio::ActionGroup;
}

impl MermanApplication {
    fn main_window(&self) -> MermanApplicationWindow {
        self.imp().window.get().unwrap().upgrade().unwrap()
    }

    fn setup_gactions(&self) {
        let action_quit = gio::ActionEntry::builder("quit")
            .activate(move |app: &Self, _, _| {
                // Closing the window first gives it a chance to save its state
                // and to ask about unsaved changes.
                app.main_window().close();
            })
            .build();

        let action_about = gio::ActionEntry::builder("about")
            .activate(|app: &Self, _, _| {
                app.show_about_dialog();
            })
            .build();
        self.add_action_entries([action_quit, action_about]);
    }

    fn setup_accels(&self) {
        self.set_accels_for_action("app.quit", &["<Control>q"]);
        self.set_accels_for_action("window.close", &["<Control>w"]);
        self.set_accels_for_action("win.new", &["<Control>n"]);
        self.set_accels_for_action("win.open", &["<Control>o"]);
        self.set_accels_for_action("win.save", &["<Control>s"]);
        self.set_accels_for_action("win.save-as", &["<Control><Shift>s"]);
    }

    fn authors() -> Vec<&'static str> {
        // Authors are defined in Cargo.toml
        env!("CARGO_PKG_AUTHORS").split(":").collect()
    }

    fn show_about_dialog(&self) {
        let dialog = adw::AboutDialog::builder()
            .application_name("Merman")
            .application_icon(*APP_ID)
            .developer_name("Bruno Paz")
            .version(*VERSION)
            .website("https://github.com/brpaz/merman")
            .issue_url("https://github.com/brpaz/merman/issues")
            .license_type(gtk::License::MitX11)
            .developers(Self::authors())
            .translator_credits(gettext("translator-credits"))
            .build();

        dialog.present(Some(&self.main_window()));
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
            .property("flags", gio::ApplicationFlags::HANDLES_OPEN)
            .property("resource-base-path", "/dev/brunopaz/Merman/")
            .build()
    }
}

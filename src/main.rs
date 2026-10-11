mod application;
mod class_diagram;
mod class_editor;
mod combo;
mod config;
mod diagram;
mod drawing;
mod erd;
mod erd_editor;
mod flowchart;
mod flowchart_editor;
mod geometry;
mod history;
mod ids;
mod interaction;
mod labels;
mod layered;
mod palette;
mod routing;
mod sequence;
mod sequence_editor;
mod syntax;
mod viewport;
mod window;

use std::path::PathBuf;

use gettextrs::{LocaleCategory, gettext};
use gtk::{gio, glib};

use self::application::MermanApplication;
use self::config::{GETTEXT_PACKAGE, LOCALEDIR, RESOURCES_FILE};

fn main() -> glib::ExitCode {
    // Initialize logger
    tracing_subscriber::fmt::init();

    // Prepare i18n
    gettextrs::setlocale(LocaleCategory::LcAll, "");
    gettextrs::bindtextdomain(*GETTEXT_PACKAGE, *LOCALEDIR)
        .expect("Unable to bind the text domain");
    gettextrs::textdomain(*GETTEXT_PACKAGE).expect("Unable to switch to the text domain");

    glib::set_application_name(&gettext("Merman"));

    let resources_file = std::env::var_os("MESON_RESOURCES_FILE")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(*RESOURCES_FILE));
    let res = gio::Resource::load(&resources_file).unwrap_or_else(|err| {
        panic!(
            "Could not load gresource file {}: {err}",
            resources_file.display()
        )
    });
    gio::resources_register(&res);

    let app = MermanApplication::default();
    app.run()
}

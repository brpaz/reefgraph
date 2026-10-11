//! Export of a canvas's diagram as an SVG or PNG image. Canvases draw the
//! same scene they show on screen, without the grid and interaction
//! feedback, into a snapshot that is replayed onto a cairo surface.

use std::fmt;

use gtk::cairo;
use gtk::prelude::*;

use crate::drawing::{Colors, rgb};
use crate::geometry::Rect;
use crate::viewport::Viewport;

/// Blank space around the diagram, in canvas units.
const MARGIN: f64 = 20.0;
/// Pixels per canvas unit in PNG images, so they stay sharp on HiDPI screens.
const PNG_SCALE: f64 = 2.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageFormat {
    Svg,
    Png,
}

impl ImageFormat {
    /// Format matching the extension of `name`; SVG unless it ends in `.png`.
    pub fn from_file_name(name: &str) -> Self {
        if name.to_ascii_lowercase().ends_with(".png") {
            Self::Png
        } else {
            Self::Svg
        }
    }
}

/// What a canvas draws and how.
#[derive(Debug, Clone, Copy)]
pub struct Scene {
    pub viewport: Viewport,
    /// Size of the drawing area in pixels.
    pub size: (f64, f64),
    pub colors: Colors,
    pub dark: bool,
    /// Whether to draw the grid, selection and interaction feedback.
    pub interactive: bool,
}

/// A canvas whose diagram can be exported.
pub trait Exportable {
    /// Canvas area covered by the diagram, or `None` when it is empty.
    fn content_bounds(&self) -> Option<Rect>;
    fn draw_scene(&self, snapshot: &gtk::Snapshot, scene: &Scene);
}

#[derive(Debug)]
pub enum ExportError {
    Empty,
    Cairo(cairo::Error),
    Write(String),
}

impl fmt::Display for ExportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ExportError::Empty => f.write_str("the diagram is empty"),
            ExportError::Cairo(err) => write!(f, "drawing failed: {err}"),
            ExportError::Write(err) => write!(f, "encoding failed: {err}"),
        }
    }
}

impl std::error::Error for ExportError {}

impl From<cairo::Error> for ExportError {
    fn from(err: cairo::Error) -> Self {
        ExportError::Cairo(err)
    }
}

/// Renders the diagram on a white background in light colors, returning
/// the encoded image.
pub fn render(canvas: &impl Exportable, format: ImageFormat) -> Result<Vec<u8>, ExportError> {
    let bounds = canvas.content_bounds().ok_or(ExportError::Empty)?;
    let bounds = bounds.inflate(MARGIN);
    let scale = match format {
        ImageFormat::Svg => 1.0,
        ImageFormat::Png => PNG_SCALE,
    };
    let size = (
        (bounds.w * scale).ceil().max(1.0),
        (bounds.h * scale).ceil().max(1.0),
    );
    let scene = Scene {
        viewport: Viewport {
            zoom: scale,
            offset: (-bounds.x * scale, -bounds.y * scale),
        },
        size,
        colors: Colors::new(rgb(0x24, 0x24, 0x24), false),
        dark: false,
        interactive: false,
    };
    let snapshot = gtk::Snapshot::new();
    canvas.draw_scene(&snapshot, &scene);
    let node = snapshot.to_node();
    let paint = |cr: &cairo::Context| -> Result<(), cairo::Error> {
        cr.set_source_rgb(1.0, 1.0, 1.0);
        cr.paint()?;
        if let Some(node) = &node {
            node.draw(cr);
        }
        cr.status()
    };

    match format {
        ImageFormat::Svg => {
            let mut surface = cairo::SvgSurface::for_stream(size.0, size.1, Vec::<u8>::new())?;
            surface.set_document_unit(cairo::SvgUnit::Px);
            paint(&cairo::Context::new(&surface)?)?;
            let stream = surface
                .finish_output_stream()
                .map_err(|err| ExportError::Write(err.error.to_string()))?;
            stream
                .downcast::<Vec<u8>>()
                .map(|bytes| *bytes)
                .map_err(|_| ExportError::Write("unexpected output stream".into()))
        }
        ImageFormat::Png => {
            let surface =
                cairo::ImageSurface::create(cairo::Format::ARgb32, size.0 as i32, size.1 as i32)?;
            paint(&cairo::Context::new(&surface)?)?;
            let mut bytes = Vec::new();
            surface
                .write_to_png(&mut bytes)
                .map_err(|err| ExportError::Write(err.to_string()))?;
            Ok(bytes)
        }
    }
}

/// Scene for drawing a canvas on screen in the current style.
pub fn screen_scene(widget: &impl IsA<gtk::Widget>, viewport: Viewport) -> Scene {
    let dark = adw::StyleManager::default().is_dark();
    Scene {
        viewport,
        size: (widget.width() as f64, widget.height() as f64),
        colors: Colors::new(widget.color(), dark),
        dark,
        interactive: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_format_from_extension() {
        assert_eq!(ImageFormat::from_file_name("a.PNG"), ImageFormat::Png);
        assert_eq!(ImageFormat::from_file_name("a.svg"), ImageFormat::Svg);
        assert_eq!(ImageFormat::from_file_name("diagram"), ImageFormat::Svg);
    }
}

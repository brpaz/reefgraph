//! C4 diagrams on the box canvas: elements are boxes, relations are links
//! and boundaries are frames drawn around their members. Row 0 of an element
//! is its technology, row 1 its description.

use std::any::Any;

use gtk::cairo;

use super::ElementPreset;
use super::render;
use crate::canvas::box_canvas::{self, BoxDiagram, Highlight, Measure};
use crate::canvas::drawing::{self, Colors, Text, log_draw_error};
use crate::diagrams::c4::layout::{self, Line};
use crate::diagrams::c4::{C4Diagram, Element, Level};
use crate::shared::geometry::{Point, Rect};
use crate::shared::ids::RenameError;
use crate::shared::routing;

fn element_rect(e: &Element, measure: Measure) -> Option<Rect> {
    Some(layout::element_box(e, measure).rect(e.pos?))
}

impl BoxDiagram for C4Diagram {
    fn clone_diagram(&self) -> Box<dyn BoxDiagram> {
        Box::new(self.clone())
    }

    fn same_as(&self, other: &dyn BoxDiagram) -> bool {
        box_canvas::same(self, other)
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn box_ids(&self) -> Vec<String> {
        self.elements.iter().map(|e| e.id.clone()).collect()
    }

    fn boxes(&self, measure: Measure) -> Vec<(&str, Rect)> {
        self.elements
            .iter()
            .filter_map(|e| Some((e.id.as_str(), element_rect(e, measure)?)))
            .collect()
    }

    fn decorations(&self, measure: Measure) -> Vec<Rect> {
        layout::frames(self, |e| element_rect(e, measure), measure)
            .into_iter()
            .map(|f| f.rect)
            .collect()
    }

    fn position(&self, id: &str) -> Option<Point> {
        self.element(id)?.pos
    }

    fn set_position(&mut self, id: &str, pos: Point) {
        if let Some(element) = self.element_mut(id) {
            element.pos = Some(pos);
        }
    }

    fn box_rect(&self, id: &str, measure: Measure) -> Option<Rect> {
        element_rect(self.element(id)?, measure)
    }

    fn label(&self, id: &str) -> Option<String> {
        Some(self.element(id)?.label.clone())
    }

    fn set_label(&mut self, id: &str, label: String) {
        if let Some(element) = self.element_mut(id) {
            element.label = label;
        }
    }

    fn header_center(&self, id: &str, measure: Measure) -> Option<Point> {
        let element = self.element(id)?;
        layout::element_box(element, measure).line_center(element.pos?, Line::Label)
    }

    fn layout(&mut self, all: bool, measure: Measure) {
        let size = |e: &Element| layout::element_box(e, measure).size;
        if all {
            layout::layout_all(self, size);
        } else {
            layout::layout_missing(self, size);
        }
    }

    fn links(&self) -> Vec<(String, String)> {
        self.relations
            .iter()
            .map(|r| (r.from.clone(), r.to.clone()))
            .collect()
    }

    fn default_preset(&self) -> &'static str {
        match self.level {
            Level::Context => "system",
            Level::Container => "container",
            Level::Component => "component",
        }
    }

    fn add_box(&mut self, preset: &str, at: Point) -> Option<String> {
        let p = ElementPreset::from_id(preset)?;
        Some(self.add_element(p.kind, p.shape, p.external, at))
    }

    fn paste_box(&mut self, from: &dyn BoxDiagram, id: &str, at: Point) -> Option<String> {
        let copied = from
            .as_any()
            .downcast_ref::<C4Diagram>()?
            .element(id)?
            .clone();
        Some(self.paste_element(&copied, at))
    }

    fn remove_box(&mut self, id: &str) {
        self.remove_element(id);
    }

    fn rename_box(&mut self, old: &str, new: &str) -> Result<(), RenameError> {
        self.rename_element(old, new)
    }

    fn add_link(&mut self, from: &str, to: &str) -> Option<usize> {
        Some(self.add_relation(from, to))
    }

    fn remove_link(&mut self, index: usize) {
        if index < self.relations.len() {
            self.relations.remove(index);
        }
    }

    fn link_label(&self, index: usize) -> Option<String> {
        Some(self.relations.get(index)?.label.clone())
    }

    fn set_link_label(&mut self, index: usize, label: String) {
        if let Some(r) = self.relations.get_mut(index) {
            r.label = label;
        }
    }

    fn row_at(&self, id: &str, p: Point, measure: Measure) -> Option<usize> {
        let element = self.element(id)?;
        layout::element_box(element, measure).row_at(element.pos?, p)
    }

    fn grows_rows(&self) -> bool {
        false
    }

    fn add_row(&mut self, _id: &str) -> Option<usize> {
        None
    }

    fn row_text(&self, id: &str, row: usize, measure: Measure) -> Option<(String, Point)> {
        let element = self.element(id)?;
        let text = match row {
            0 if element.kind.has_technology() => element.technology.clone(),
            1 => element.description.clone(),
            _ => return None,
        };
        let at = layout::element_box(element, measure).row_anchor(element.pos?, row);
        Some((text, at))
    }

    fn set_row_text(&mut self, id: &str, row: usize, text: &str) -> Result<(), String> {
        if let Some(element) = self.element_mut(id) {
            let text = text.trim().to_string();
            match row {
                0 if element.kind.has_technology() => element.technology = text,
                1 => element.description = text,
                _ => {}
            }
        }
        Ok(())
    }

    fn draw(
        &self,
        cr: &cairo::Context,
        paths: &[Vec<Point>],
        highlight: &Highlight,
        colors: &Colors,
        measure: Measure,
    ) -> Vec<Text> {
        let avoid: Vec<Rect> = self.boxes(measure).into_iter().map(|(_, r)| r).collect();
        let mut texts = Vec::new();
        for frame in layout::frames(self, |e| element_rect(e, measure), measure) {
            log_draw_error(render::draw_frame(cr, &frame.rect, colors));
            for (i, (text, left)) in frame.lines.into_iter().enumerate() {
                let (w, _) = measure(&text);
                texts.push(Text {
                    bold: i == 0,
                    ..Text::plain(text, Point::new(left.x + w / 2.0, left.y), colors.fg)
                });
            }
        }

        for (i, (r, path)) in self.relations.iter().zip(paths).enumerate() {
            log_draw_error(render::draw_relation(
                cr,
                path,
                r.kind == crate::diagrams::c4::RelationKind::BiRel,
                colors,
                highlight.link == Some(i),
            ));
            if path.is_empty() {
                continue;
            }
            let technology = if r.technology.is_empty() {
                String::new()
            } else {
                format!("[{}]", r.technology)
            };
            let lines: Vec<&str> = [r.label.as_str(), technology.as_str()]
                .into_iter()
                .filter(|t| !t.is_empty())
                .collect();
            if lines.is_empty() {
                continue;
            }
            let sizes: Vec<_> = lines.iter().map(|t| measure(t)).collect();
            let w = sizes.iter().map(|s| s.0).fold(0.0, f64::max);
            let h: f64 = sizes.iter().map(|s| s.1).sum();
            let mid = routing::label_position(path, (w + 8.0, h + 4.0), &avoid);
            log_draw_error(drawing::draw_label_background(
                cr,
                &Rect::centered(mid, (w + 8.0, h + 4.0)),
                colors,
            ));
            let mut y = mid.y - h / 2.0;
            for (i, (text, (_, lh))) in lines.into_iter().zip(sizes).enumerate() {
                texts.push(Text {
                    italic: i == 1 || r.label.is_empty(),
                    ..Text::plain(text, Point::new(mid.x, y + lh / 2.0), colors.fg)
                });
                y += lh;
            }
        }

        for element in &self.elements {
            let Some(pos) = element.pos else {
                continue;
            };
            let layout = layout::element_box(element, measure);
            let rect = layout.rect(pos);
            log_draw_error(render::draw_element(
                cr,
                element,
                &rect,
                colors,
                highlight.boxes.contains(&element.id),
            ));
            let (_, text_color) = render::element_colors(element);
            for (line, text, y) in layout.lines {
                texts.push(Text {
                    bold: line == Line::Label,
                    italic: matches!(line, Line::Stereotype | Line::Technology),
                    ..Text::plain(text, Point::new(rect.center().x, rect.y + y), text_color)
                });
            }
        }
        texts
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagrams::c4::ElementKind;

    fn size(s: &str) -> (f64, f64) {
        (s.chars().count() as f64 * 8.0, 16.0)
    }

    #[test]
    fn edits_technology_only_on_kinds_that_have_one() {
        let mut d = crate::diagrams::c4::parse(
            "C4Container\nSystem(s, \"S\")\nContainer(c, \"C\", \"Go\", \"API\")",
        )
        .unwrap();
        d.layout(false, &size);
        assert_eq!(d.row_text("s", 0, &size), None);
        assert_eq!(d.row_text("c", 0, &size).unwrap().0, "Go");
        d.set_row_text("c", 1, " Serves JSON ").unwrap();
        assert_eq!(d.element("c").unwrap().description, "Serves JSON");
        assert_eq!(d.element("c").unwrap().kind, ElementKind::Container);
    }
}

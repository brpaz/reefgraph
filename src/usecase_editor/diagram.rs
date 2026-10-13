//! Use case diagrams on the box canvas: actors and use cases are boxes
//! without rows, relationships are links and system boundaries are frames
//! drawn around their members.

use std::any::Any;

use gtk::cairo;

use super::{preset_id, preset_kind, render};
use crate::box_canvas::{self, BoxDiagram, Highlight, Measure};
use crate::drawing::{self, Colors, Text, log_draw_error};
use crate::geometry::{Point, Rect};
use crate::ids::RenameError;
use crate::routing::{self, Outline};
use crate::usecase::layout::{self, element_size, text_center, text_lines};
use crate::usecase::{Element, ElementKind, Label, RelationKind, UseCaseDiagram};

fn element_rect(e: &Element, measure: Measure) -> Option<Rect> {
    Some(Rect::centered(e.pos?, element_size(e, measure)))
}

impl BoxDiagram for UseCaseDiagram {
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
        if let Some(e) = self.element_mut(id) {
            e.pos = Some(pos);
        }
    }

    fn box_rect(&self, id: &str, measure: Measure) -> Option<Rect> {
        element_rect(self.element(id)?, measure)
    }

    fn outline(&self, id: &str) -> Outline {
        match self.element(id).map(|e| e.kind) {
            Some(ElementKind::UseCase) => Outline::Ellipse,
            Some(ElementKind::Actor | ElementKind::UseCaseRect) | None => Outline::Rect,
        }
    }

    fn label(&self, id: &str) -> Option<String> {
        Some(self.element(id)?.name())
    }

    fn set_label(&mut self, id: &str, label: String) {
        if let Some(e) = self.element_mut(id) {
            e.label = if label == e.id {
                Label::default()
            } else {
                Label::from_text(&label)
            };
        }
    }

    fn header_center(&self, id: &str, measure: Measure) -> Option<Point> {
        let e = self.element(id)?;
        Some(text_center(e, &element_rect(e, measure)?, measure))
    }

    fn layout(&mut self, all: bool, measure: Measure) {
        let size = |e: &Element| element_size(e, measure);
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
        preset_id(ElementKind::UseCase)
    }

    fn add_box(&mut self, preset: &str, at: Point) -> Option<String> {
        Some(self.add_element(preset_kind(preset)?, at))
    }

    fn remove_box(&mut self, id: &str) {
        self.remove_element(id);
    }

    fn paste_box(&mut self, from: &dyn BoxDiagram, id: &str, at: Point) -> Option<String> {
        let copied = from
            .as_any()
            .downcast_ref::<UseCaseDiagram>()?
            .element(id)?
            .clone();
        Some(self.paste_element(&copied, at))
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

    /// Only associations carry a label; the others show their kind.
    fn link_label(&self, index: usize) -> Option<String> {
        let r = self.relations.get(index)?;
        r.kind.is_labelled().then(|| r.label.display(""))
    }

    fn set_link_label(&mut self, index: usize, label: String) {
        if let Some(r) = self.relations.get_mut(index) {
            r.label = Label::from_text(&label);
        }
    }

    fn row_at(&self, _id: &str, _p: Point, _measure: Measure) -> Option<usize> {
        None
    }

    fn grows_rows(&self) -> bool {
        false
    }

    fn add_row(&mut self, _id: &str) -> Option<usize> {
        None
    }

    fn row_text(&self, _id: &str, _row: usize, _measure: Measure) -> Option<(String, Point)> {
        None
    }

    fn set_row_text(&mut self, _id: &str, _row: usize, _text: &str) -> Result<(), String> {
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
        let dim = colors.fg.with_alpha(0.65);
        for frame in layout::frames(self, |e| element_rect(e, measure), measure) {
            log_draw_error(render::draw_frame(cr, &frame.rect, colors));
            texts.push(Text {
                bold: true,
                ..Text::plain(frame.title, frame.title_at, colors.fg)
            });
        }

        for (i, (r, path)) in self.relations.iter().zip(paths).enumerate() {
            log_draw_error(render::draw_relation(
                cr,
                path,
                r.kind,
                colors,
                highlight.link == Some(i),
            ));
            let text = match r.kind {
                RelationKind::Include => "«include»".to_string(),
                RelationKind::Extend => "«extend»".to_string(),
                RelationKind::Line | RelationKind::Arrow => r.label.display(""),
                RelationKind::ArrowBack
                | RelationKind::CircleTo
                | RelationKind::CircleFrom
                | RelationKind::CrossTo
                | RelationKind::CrossFrom
                | RelationKind::Generalization => String::new(),
            };
            if path.len() >= 2 && !text.is_empty() {
                let (w, h) = measure(&text);
                let size = (w + 8.0, h + 4.0);
                let at = routing::label_position(path, size, &avoid);
                log_draw_error(drawing::draw_label_background(
                    cr,
                    &Rect::centered(at, size),
                    colors,
                ));
                texts.push(Text::plain(text, at, colors.fg));
            }
        }

        for e in &self.elements {
            let Some(rect) = element_rect(e, measure) else {
                continue;
            };
            log_draw_error(render::draw_element(
                cr,
                &rect,
                e.kind,
                colors,
                highlight.boxes.contains(&e.id),
            ));
            let lines = text_lines(e);
            let heights: Vec<f64> = lines.iter().map(|t| measure(t).1).collect();
            let center = text_center(e, &rect, measure);
            let mut y = center.y - heights.iter().sum::<f64>() / 2.0;
            let stereotype = lines.len() > 1;
            for (i, (line, h)) in lines.into_iter().zip(heights).enumerate() {
                let color = if stereotype && i == 0 { dim } else { colors.fg };
                texts.push(Text::plain(line, Point::new(center.x, y + h / 2.0), color));
                y += h;
            }
        }
        texts
    }
}

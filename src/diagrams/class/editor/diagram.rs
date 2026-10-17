//! Class diagrams on the box canvas: classes are boxes, their members are
//! rows and relations are links. Row indices are indices into
//! `Class::members`, whatever compartment the member is shown in.

use std::any::Any;

use gtk::cairo;

use super::ClassPreset;
use super::render;
use crate::canvas::box_canvas::{self, BoxDiagram, Highlight, Measure};
use crate::canvas::drawing::{self, Colors, Text, log_draw_error};
use crate::diagrams::class::layout::{self, annotation_text};
use crate::diagrams::class::{Class, ClassDiagram, Member};
use crate::shared::geometry::{Point, Rect};
use crate::shared::ids::RenameError;
use crate::shared::routing;

impl BoxDiagram for ClassDiagram {
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
        self.classes.iter().map(|c| c.id.clone()).collect()
    }

    fn position(&self, id: &str) -> Option<Point> {
        self.class(id)?.pos
    }

    fn set_position(&mut self, id: &str, pos: Point) {
        if let Some(class) = self.class_mut(id) {
            class.pos = Some(pos);
        }
    }

    fn box_rect(&self, id: &str, measure: Measure) -> Option<Rect> {
        let class = self.class(id)?;
        Some(layout::class_box(class, measure).rect(class.pos?))
    }

    fn boxes(&self, measure: Measure) -> Vec<(&str, Rect)> {
        self.classes
            .iter()
            .filter_map(|c| Some((c.id.as_str(), layout::class_box(c, measure).rect(c.pos?))))
            .collect()
    }

    fn label(&self, id: &str) -> Option<String> {
        Some(self.class(id)?.label.clone())
    }

    fn set_label(&mut self, id: &str, label: String) {
        if let Some(class) = self.class_mut(id) {
            class.label = label;
        }
    }

    fn header_center(&self, id: &str, measure: Measure) -> Option<Point> {
        let class = self.class(id)?;
        let layout = layout::class_box(class, measure);
        let rect = layout.rect(class.pos?);
        Some(Point::new(
            rect.center().x,
            rect.y + layout.header_height / 2.0,
        ))
    }

    fn layout(&mut self, all: bool, measure: Measure) {
        let size = |c: &Class| layout::class_box(c, measure).size;
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
        ClassPreset::Class.id()
    }

    fn add_box(&mut self, preset: &str, at: Point) -> Option<String> {
        let preset = ClassPreset::from_id(preset)?;
        Some(self.add_class(preset.annotations(), at))
    }

    fn paste_box(&mut self, from: &dyn BoxDiagram, id: &str, at: Point) -> Option<String> {
        let copied = from
            .as_any()
            .downcast_ref::<ClassDiagram>()?
            .class(id)?
            .clone();
        Some(self.paste_class(&copied, at))
    }

    fn remove_box(&mut self, id: &str) {
        self.remove_class(id);
    }

    fn rename_box(&mut self, old: &str, new: &str) -> Result<(), RenameError> {
        self.rename_class(old, new)
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
        let class = self.class(id)?;
        let row = layout::class_box(class, measure).row_at(class.pos?, p)?;
        class.display_order().get(row).copied()
    }

    fn add_row(&mut self, id: &str) -> Option<usize> {
        let class = self.class_mut(id)?;
        class.members.push(Member("+field".into()));
        Some(class.members.len() - 1)
    }

    fn row_text(&self, id: &str, index: usize, measure: Measure) -> Option<(String, Point)> {
        let class = self.class(id)?;
        let text = class.members.get(index)?.0.clone();
        let row = class.display_order().iter().position(|&i| i == index)?;
        let at = layout::class_box(class, measure)
            .row_rect(class.pos?, row)
            .center();
        Some((text, at))
    }

    /// Clearing a member's text removes it.
    fn set_row_text(&mut self, id: &str, index: usize, text: &str) -> Result<(), String> {
        if let Some(class) = self.class_mut(id)
            && index < class.members.len()
        {
            if text.is_empty() {
                class.members.remove(index);
            } else {
                class.members[index] = Member(text.to_string());
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
        for (i, (r, path)) in self.relations.iter().zip(paths).enumerate() {
            log_draw_error(render::draw_relation(
                cr,
                path,
                r.from_head,
                r.to_head,
                r.dashed,
                colors,
                highlight.link == Some(i),
            ));
            let n = path.len();
            if n < 2 {
                continue;
            }
            if !r.label.is_empty() {
                let (w, h) = measure(&r.label);
                let size = (w + 8.0, h + 4.0);
                let at = routing::label_position(path, size, &avoid);
                log_draw_error(drawing::draw_label_background(
                    cr,
                    &Rect::centered(at, size),
                    colors,
                ));
                texts.push(Text::plain(r.label.clone(), at, colors.fg));
            }
            for (cardinality, at, toward) in [
                (&r.from_cardinality, path[0], path[1]),
                (&r.to_cardinality, path[n - 1], path[n - 2]),
            ] {
                if !cardinality.is_empty() {
                    texts.push(Text::plain(
                        cardinality.clone(),
                        render::cardinality_position(at, toward),
                        colors.fg,
                    ));
                }
            }
        }

        let dim = colors.fg.with_alpha(0.65);
        for class in &self.classes {
            let Some(pos) = class.pos else {
                continue;
            };
            let layout = layout::class_box(class, measure);
            let rect = layout.rect(pos);
            log_draw_error(render::draw_class(
                cr,
                &rect,
                &layout,
                colors,
                highlight.boxes.contains(&class.id),
            ));

            let mut y = rect.y + layout::HEADER_PADDING / 2.0;
            for annotation in &class.annotations {
                let text = annotation_text(annotation);
                let (_, h) = measure(&text);
                texts.push(Text::plain(
                    text,
                    Point::new(rect.center().x, y + h / 2.0),
                    dim,
                ));
                y += h;
            }
            let title = class.title();
            let (_, h) = measure(&title);
            texts.push(Text {
                bold: true,
                ..Text::plain(title, Point::new(rect.center().x, y + h / 2.0), colors.fg)
            });

            for (row, index) in class.display_order().into_iter().enumerate() {
                let display = class.members[index].display();
                let (w, _) = measure(&display.text);
                let y = layout.row_rect(pos, row).center().y;
                texts.push(Text {
                    italic: display.is_abstract,
                    underline: display.is_static,
                    ..Text::plain(
                        display.text,
                        Point::new(rect.x + layout::PADDING + w / 2.0, y),
                        colors.fg,
                    )
                });
            }
        }
        texts
    }
}

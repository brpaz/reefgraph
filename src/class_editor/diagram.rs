//! Class diagrams on the box canvas: classes are boxes, their members are
//! rows and relations are links. Row indices are indices into
//! `Class::members`, whatever compartment the member is shown in.

use std::any::Any;

use gtk::cairo;

use super::ClassPreset;
use super::render;
use crate::box_canvas::{self, BoxDiagram, EditTarget, Highlight, Measure};
use crate::class_diagram::layout::{self, ClassBox, annotation_text};
use crate::class_diagram::{Class, ClassDiagram, Member};
use crate::drawing::{self, Colors, Text, log_draw_error};
use crate::geometry::{Point, Rect};
use crate::ids::RenameError;
use crate::routing;

fn class_box(class: &Class, measure: Measure) -> ClassBox {
    layout::class_box(class, measure)
}

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
        Some(class_box(class, measure).rect(class.pos?))
    }

    fn layout(&mut self, all: bool, measure: Measure) {
        let size = |c: &Class| class_box(c, measure).size;
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

    fn add_box(&mut self, preset: &str, at: Point) -> Option<String> {
        // An empty preset is a plain class, e.g. from double-clicking the canvas.
        let preset = if preset.is_empty() {
            ClassPreset::Class
        } else {
            ClassPreset::from_id(preset)?
        };
        Some(self.add_class(preset.annotations(), at))
    }

    fn remove_box(&mut self, id: &str) {
        self.remove_class(id);
    }

    fn rename_box(&mut self, old: &str, new: &str) -> Result<(), RenameError> {
        self.rename_class(old, new)
    }

    fn add_link(&mut self, from: &str, to: &str) -> usize {
        self.add_relation(from, to)
    }

    fn remove_link(&mut self, index: usize) {
        if index < self.relations.len() {
            self.relations.remove(index);
        }
    }

    fn row_at(&self, id: &str, p: Point, measure: Measure) -> Option<usize> {
        let class = self.class(id)?;
        let row = class_box(class, measure).row_at(class.pos?, p)?;
        class.display_order().get(row).copied()
    }

    fn add_row(&mut self, id: &str) -> Option<usize> {
        let class = self.class_mut(id)?;
        class.members.push(Member("+field".into()));
        Some(class.members.len() - 1)
    }

    fn edit_text(&self, target: &EditTarget, measure: Measure) -> Option<(String, Point)> {
        match target {
            EditTarget::Name(id) => {
                let class = self.class(id)?;
                let layout = class_box(class, measure);
                let rect = layout.rect(class.pos?);
                let at = Point::new(rect.center().x, rect.y + layout.header_height / 2.0);
                Some((class.label.clone(), at))
            }
            EditTarget::Row(id, index) => {
                let class = self.class(id)?;
                let text = class.members.get(*index)?.0.clone();
                let row = class.display_order().iter().position(|i| i == index)?;
                let at = class_box(class, measure).row_rect(class.pos?, row).center();
                Some((text, at))
            }
            EditTarget::Link(i) => Some((self.relations.get(*i)?.label.clone(), Point::default())),
        }
    }

    fn apply_text(&mut self, target: &EditTarget, text: &str) -> Result<(), String> {
        match target {
            // Classes without a label show their name, so editing it renames.
            EditTarget::Name(id) => {
                let has_label = self.class(id).is_some_and(|c| c.label != c.id);
                if !has_label {
                    return self.rename_class(id, text).map_err(|e| e.to_string());
                }
                if let Some(class) = self.class_mut(id) {
                    class.label = if text.is_empty() {
                        id.clone()
                    } else {
                        text.to_string()
                    };
                }
                Ok(())
            }
            // Clearing a member's text removes it.
            EditTarget::Row(id, index) => {
                if let Some(class) = self.class_mut(id)
                    && *index < class.members.len()
                {
                    if text.is_empty() {
                        class.members.remove(*index);
                    } else {
                        class.members[*index] = Member(text.to_string());
                    }
                }
                Ok(())
            }
            EditTarget::Link(i) => {
                if let Some(r) = self.relations.get_mut(*i) {
                    r.label = text.to_string();
                }
                Ok(())
            }
        }
    }

    fn draw(
        &self,
        cr: &cairo::Context,
        paths: &[Vec<Point>],
        highlight: &Highlight,
        colors: &Colors,
        measure: Measure,
    ) -> Vec<Text> {
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
                let at = routing::path_midpoint(path);
                let (w, h) = measure(&r.label);
                log_draw_error(drawing::draw_label_background(
                    cr,
                    &Rect::centered(at, (w + 8.0, h + 4.0)),
                    colors,
                ));
                texts.push(Text::plain(r.label.clone(), at, colors.fg));
            }
            for (cardinality, at, toward) in [
                (&r.from_cardinality, path[0], path[1]),
                (&r.to_cardinality, path[n - 1], path[n - 2]),
            ] {
                texts.push(Text::plain(
                    cardinality.clone(),
                    render::cardinality_position(at, toward),
                    colors.fg,
                ));
            }
        }

        let dim = colors.fg.with_alpha(0.65);
        for class in &self.classes {
            let Some(pos) = class.pos else {
                continue;
            };
            let layout = class_box(class, measure);
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

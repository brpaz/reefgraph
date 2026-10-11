//! Entity relationship diagrams on the box canvas: entities are boxes, their
//! attributes are rows and relationships are links.

use std::any::Any;

use gtk::cairo;

use super::EntityPreset;
use super::render;
use crate::box_canvas::{self, BoxDiagram, Highlight, Measure};
use crate::drawing::{self, Colors, Text, log_draw_error};
use crate::erd::layout::{self, attribute_cells};
use crate::erd::{Attribute, Entity, Erd};
use crate::geometry::{Point, Rect};
use crate::ids::RenameError;
use crate::routing;

impl BoxDiagram for Erd {
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
        self.entities.iter().map(|e| e.id.clone()).collect()
    }

    fn position(&self, id: &str) -> Option<Point> {
        self.entity(id)?.pos
    }

    fn set_position(&mut self, id: &str, pos: Point) {
        if let Some(entity) = self.entity_mut(id) {
            entity.pos = Some(pos);
        }
    }

    fn box_rect(&self, id: &str, measure: Measure) -> Option<Rect> {
        let entity = self.entity(id)?;
        Some(layout::entity_box(entity, measure).rect(entity.pos?))
    }

    fn boxes(&self, measure: Measure) -> Vec<(&str, Rect)> {
        self.entities
            .iter()
            .filter_map(|e| Some((e.id.as_str(), layout::entity_box(e, measure).rect(e.pos?))))
            .collect()
    }

    fn label(&self, id: &str) -> Option<String> {
        Some(self.entity(id)?.label.clone())
    }

    fn set_label(&mut self, id: &str, label: String) {
        if let Some(entity) = self.entity_mut(id) {
            entity.label = label;
        }
    }

    fn header_center(&self, id: &str, measure: Measure) -> Option<Point> {
        let entity = self.entity(id)?;
        let layout = layout::entity_box(entity, measure);
        let rect = layout.rect(entity.pos?);
        Some(Point::new(
            rect.center().x,
            rect.y + layout.header_height / 2.0,
        ))
    }

    fn layout(&mut self, all: bool, measure: Measure) {
        let size = |e: &Entity| layout::entity_box(e, measure).size;
        if all {
            layout::layout_all(self, size);
        } else {
            layout::layout_missing(self, size);
        }
    }

    fn links(&self) -> Vec<(String, String)> {
        self.relationships
            .iter()
            .map(|r| (r.from.clone(), r.to.clone()))
            .collect()
    }

    fn default_preset(&self) -> &'static str {
        EntityPreset::Empty.id()
    }

    fn add_box(&mut self, preset: &str, at: Point) -> Option<String> {
        let preset = EntityPreset::from_id(preset)?;
        Some(self.add_entity(preset.attributes(), at))
    }

    fn paste_box(&mut self, from: &dyn BoxDiagram, id: &str, at: Point) -> Option<String> {
        let copied = from.as_any().downcast_ref::<Erd>()?.entity(id)?.clone();
        Some(self.paste_entity(&copied, at))
    }

    fn remove_box(&mut self, id: &str) {
        self.remove_entity(id);
    }

    fn rename_box(&mut self, old: &str, new: &str) -> Result<(), RenameError> {
        self.rename_entity(old, new)
    }

    fn add_link(&mut self, from: &str, to: &str) -> usize {
        self.add_relationship(from, to)
    }

    fn remove_link(&mut self, index: usize) {
        if index < self.relationships.len() {
            self.relationships.remove(index);
        }
    }

    fn link_label(&self, index: usize) -> Option<String> {
        Some(self.relationships.get(index)?.label.clone())
    }

    fn set_link_label(&mut self, index: usize, label: String) {
        if let Some(r) = self.relationships.get_mut(index) {
            r.label = label;
        }
    }

    fn row_at(&self, id: &str, p: Point, measure: Measure) -> Option<usize> {
        let entity = self.entity(id)?;
        let row = layout::entity_box(entity, measure).row_at(entity.pos?, p)?;
        (row < entity.attributes.len()).then_some(row)
    }

    fn add_row(&mut self, id: &str) -> Option<usize> {
        let entity = self.entity_mut(id)?;
        entity
            .attributes
            .push(Attribute::new("string", "attribute"));
        Some(entity.attributes.len() - 1)
    }

    fn row_text(&self, id: &str, row: usize, measure: Measure) -> Option<(String, Point)> {
        let entity = self.entity(id)?;
        let text = entity.attributes.get(row)?.to_line();
        let at = layout::entity_box(entity, measure)
            .row_rect(entity.pos?, row)
            .center();
        Some((text, at))
    }

    fn set_row_text(&mut self, id: &str, row: usize, text: &str) -> Result<(), String> {
        let attribute = Attribute::parse(text)?;
        if let Some(slot) = self.entity_mut(id).and_then(|e| e.attributes.get_mut(row)) {
            *slot = attribute;
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
        let mut texts = Vec::new();
        for (i, (r, path)) in self.relationships.iter().zip(paths).enumerate() {
            log_draw_error(render::draw_relationship(
                cr,
                path,
                r.from_cardinality,
                r.to_cardinality,
                r.identifying,
                colors,
                highlight.link == Some(i),
            ));
            if !r.label.is_empty() && !path.is_empty() {
                let at = routing::path_midpoint(path);
                let (w, h) = measure(&r.label);
                log_draw_error(drawing::draw_label_background(
                    cr,
                    &Rect::centered(at, (w + 8.0, h + 4.0)),
                    colors,
                ));
                texts.push(Text::plain(r.label.clone(), at, colors.fg));
            }
        }

        let dim = colors.fg.with_alpha(0.6);
        for entity in &self.entities {
            let Some(pos) = entity.pos else {
                continue;
            };
            let layout = layout::entity_box(entity, measure);
            let rect = layout.rect(pos);
            log_draw_error(render::draw_entity(
                cr,
                &rect,
                &layout,
                entity.attributes.len(),
                colors,
                highlight.boxes.contains(&entity.id),
            ));
            texts.push(Text::plain(
                entity.label.clone(),
                Point::new(rect.center().x, rect.y + layout.header_height / 2.0),
                colors.fg,
            ));
            for row in 0..entity.attributes.len() {
                let y = layout.row_rect(pos, row).center().y;
                for (col, cell) in attribute_cells(entity, row).into_iter().enumerate() {
                    if cell.is_empty() {
                        continue;
                    }
                    let (w, _) = measure(&cell);
                    let x = rect.x + layout.columns[col] + w / 2.0;
                    let color = if col == 3 { dim } else { colors.fg };
                    texts.push(Text::plain(cell, Point::new(x, y), color));
                }
            }
        }
        texts
    }
}

//! State diagrams on the box canvas: states and pseudo-states are boxes
//! without rows and transitions are links.

use std::any::Any;

use gtk::cairo;

use super::{preset_kind, render};
use crate::canvas::box_canvas::{self, BoxDiagram, Highlight, Measure};
use crate::canvas::drawing::{self, Colors, Text, log_draw_error};
use crate::diagrams::state::layout;
use crate::diagrams::state::{State, StateDiagram, StateKind};
use crate::shared::geometry::{Point, Rect};
use crate::shared::ids::RenameError;
use crate::shared::routing::{self, Outline};

impl StateDiagram {
    fn rect_of(&self, state: &State, measure: Measure) -> Option<Rect> {
        layout::state_rect(state, self.direction.unwrap_or_default(), measure)
    }
}

impl BoxDiagram for StateDiagram {
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
        self.states.iter().map(|s| s.id.clone()).collect()
    }

    fn boxes(&self, measure: Measure) -> Vec<(&str, Rect)> {
        self.states
            .iter()
            .filter_map(|s| Some((s.id.as_str(), self.rect_of(s, measure)?)))
            .collect()
    }

    fn position(&self, id: &str) -> Option<Point> {
        self.state(id)?.pos
    }

    fn set_position(&mut self, id: &str, pos: Point) {
        if let Some(state) = self.state_mut(id) {
            state.pos = Some(pos);
        }
    }

    fn box_rect(&self, id: &str, measure: Measure) -> Option<Rect> {
        self.rect_of(self.state(id)?, measure)
    }

    fn outline(&self, id: &str) -> Outline {
        match self.state(id).map(|s| s.kind) {
            Some(StateKind::Start | StateKind::End) => Outline::Ellipse,
            Some(StateKind::Choice) => Outline::Diamond,
            Some(StateKind::State | StateKind::Fork | StateKind::Join) | None => Outline::Rect,
        }
    }

    /// Start and end states have no name to edit; pseudo-states are renamed.
    fn label(&self, id: &str) -> Option<String> {
        let state = self.state(id)?;
        match state.kind {
            StateKind::Start | StateKind::End => None,
            StateKind::State => Some(state.label.clone()),
            StateKind::Choice | StateKind::Fork | StateKind::Join => Some(state.id.clone()),
        }
    }

    fn set_label(&mut self, id: &str, label: String) {
        if let Some(state) = self.state_mut(id)
            && state.kind == StateKind::State
        {
            state.label = label;
        }
    }

    fn header_center(&self, id: &str, measure: Measure) -> Option<Point> {
        Some(self.box_rect(id, measure)?.center())
    }

    fn layout(&mut self, all: bool, measure: Measure) {
        if all {
            layout::layout_all(self, measure);
        } else {
            layout::layout_missing(self, measure);
        }
    }

    fn links(&self) -> Vec<(String, String)> {
        self.transitions
            .iter()
            .map(|t| (t.from.clone(), t.to.clone()))
            .collect()
    }

    fn default_preset(&self) -> &'static str {
        super::preset_id(StateKind::State)
    }

    fn add_box(&mut self, preset: &str, at: Point) -> Option<String> {
        self.add_state(preset_kind(preset)?, at)
    }

    fn remove_box(&mut self, id: &str) {
        self.remove_state(id);
    }

    fn paste_box(&mut self, from: &dyn BoxDiagram, id: &str, at: Point) -> Option<String> {
        let copied = from
            .as_any()
            .downcast_ref::<StateDiagram>()?
            .state(id)?
            .clone();
        self.paste_state(&copied, at)
    }

    fn rename_box(&mut self, old: &str, new: &str) -> Result<(), RenameError> {
        self.rename_state(old, new)
    }

    fn add_link(&mut self, from: &str, to: &str) -> Option<usize> {
        self.add_transition(from, to)
    }

    fn remove_link(&mut self, index: usize) {
        if index < self.transitions.len() {
            self.transitions.remove(index);
        }
    }

    fn link_label(&self, index: usize) -> Option<String> {
        Some(self.transitions.get(index)?.label.clone())
    }

    fn set_link_label(&mut self, index: usize, label: String) {
        if let Some(t) = self.transitions.get_mut(index) {
            t.label = label;
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
        let mut texts = Vec::new();
        let boxes: Vec<Rect> = self
            .states
            .iter()
            .filter_map(|s| self.rect_of(s, measure))
            .collect();
        for (i, (t, path)) in self.transitions.iter().zip(paths).enumerate() {
            log_draw_error(render::draw_transition(
                cr,
                path,
                colors,
                highlight.link == Some(i),
            ));
            if path.len() >= 2 && !t.label.is_empty() {
                let (w, h) = measure(&t.label);
                let size = (w + 8.0, h + 4.0);
                let at = routing::label_position(path, size, &boxes);
                log_draw_error(drawing::draw_label_background(
                    cr,
                    &Rect::centered(at, size),
                    colors,
                ));
                texts.push(Text::plain(t.label.clone(), at, colors.fg));
            }
        }

        for state in &self.states {
            let Some(rect) = self.rect_of(state, measure) else {
                continue;
            };
            log_draw_error(render::draw_state(
                cr,
                &rect,
                state.kind,
                colors,
                highlight.boxes.contains(&state.id),
            ));
            if state.kind == StateKind::State {
                texts.push(Text::plain(state.label.clone(), rect.center(), colors.fg));
            }
        }
        texts
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::diagrams::state::parse;

    fn size(s: &str) -> (f64, f64) {
        (s.chars().count() as f64 * 8.0, 16.0)
    }

    #[test]
    fn transition_labels_stay_off_states() {
        let mut d = parse(
            "stateDiagram-v2
    [*] --> Draft
    Draft --> Review : submit
    state check <<choice>>
    Review --> check
    check --> Draft : changes requested
    check --> Approved : approved
    Approved --> Published : publish",
        )
        .unwrap();
        BoxDiagram::layout(&mut d, true, &size);
        let boxes = d.boxes(&size);
        let nodes: HashMap<&str, (Outline, Rect)> = boxes
            .iter()
            .map(|(id, rect)| (*id, (d.outline(id), *rect)))
            .collect();
        let links = d.links();
        let edges: Vec<(&str, &str)> = links
            .iter()
            .map(|(a, b)| (a.as_str(), b.as_str()))
            .collect();
        let paths = routing::route_edges(&nodes, &edges);
        let rects: Vec<Rect> = boxes.iter().map(|(_, r)| *r).collect();
        for (t, path) in d.transitions.iter().zip(&paths) {
            if t.label.is_empty() {
                continue;
            }
            let (w, h) = size(&t.label);
            let label = Rect::centered(
                routing::label_position(path, (w + 8.0, h + 4.0), &rects),
                (w + 8.0, h + 4.0),
            );
            for (id, r) in &boxes {
                let overlaps = label.x < r.x + r.w
                    && r.x < label.x + label.w
                    && label.y < r.y + r.h
                    && r.y < label.y + label.h;
                assert!(!overlaps, "label {:?} covers state {id}", t.label);
            }
        }
    }
}

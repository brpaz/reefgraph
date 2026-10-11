//! Geometry of a sequence diagram: participant columns side by side and one
//! row per item, top to bottom. Fully determined by the model, so nothing is
//! stored in the file.

use super::model::{BlockKind, Item, NotePlacement, ParticipantKind, Sequence};
use crate::geometry::{Point, Rect};

const MARGIN: f64 = 40.0;
const PARTICIPANT_PADDING: (f64, f64) = (24.0, 12.0);
const MIN_PARTICIPANT_SIZE: (f64, f64) = (90.0, 40.0);
pub const ACTOR_FIGURE_HEIGHT: f64 = 44.0;
const COLUMN_GAP: f64 = 50.0;
/// Space kept around message labels between two lifelines.
const LABEL_GAP: f64 = 40.0;
const ROW_GAP: f64 = 16.0;
pub const SELF_LOOP_SIZE: (f64, f64) = (36.0, 22.0);
const NOTE_PADDING: (f64, f64) = (10.0, 8.0);
const NOTE_MIN_WIDTH: f64 = 80.0;
const NOTE_OFFSET: f64 = 10.0;
pub const ACTIVATION_WIDTH: f64 = 10.0;
const ACTIVATION_NESTING: f64 = 5.0;
pub const BLOCK_HEADER: f64 = 24.0;
const BLOCK_PADDING: f64 = 16.0;
const BLOCK_NESTING: f64 = 12.0;
const MARKER_ROW: f64 = 16.0;
const LIFELINE_HIT: f64 = 10.0;
const FRAME_EDGE_HIT: f64 = 6.0;

#[derive(Debug, Clone, PartialEq)]
pub struct Column {
    pub center: f64,
    pub head: Rect,
    pub foot: Rect,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RowShape {
    Message {
        y: f64,
        x1: f64,
        x2: f64,
        self_loop: bool,
        number: Option<usize>,
        label: Point,
    },
    Note(Rect),
    /// Block start, branch or end, drawn as part of its [`Frame`].
    Marker {
        y: f64,
    },
    /// Standalone `activate` / `deactivate`.
    Activation {
        y: f64,
        x: f64,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub top: f64,
    pub bottom: f64,
    pub shape: RowShape,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ActivationBar {
    pub participant: usize,
    pub rect: Rect,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Frame {
    pub kind: BlockKind,
    /// Item indices of the block's start and end markers.
    pub start: usize,
    pub end: usize,
    /// Item index and y of each branch separator.
    pub branches: Vec<(usize, f64)>,
    pub rect: Rect,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Layout {
    pub title: Option<Point>,
    pub columns: Vec<Column>,
    /// One row per item, indexed like `Sequence::items`.
    pub rows: Vec<Row>,
    pub activations: Vec<ActivationBar>,
    pub frames: Vec<Frame>,
    pub lifeline_top: f64,
    pub lifeline_bottom: f64,
    pub bounds: Rect,
}

/// What lies under a point.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hit {
    Participant(usize),
    Item(usize),
    Lifeline(usize),
}

struct OpenFrame {
    start: usize,
    kind: BlockKind,
    top: f64,
    branches: Vec<(usize, f64)>,
    /// Horizontal extent of the contents, if any.
    extent: Option<(f64, f64)>,
    /// Number of nesting levels inside this block.
    levels: usize,
}

impl OpenFrame {
    fn include(&mut self, lo: f64, hi: f64) {
        self.extent = Some(match self.extent {
            Some((a, b)) => (a.min(lo), b.max(hi)),
            None => (lo, hi),
        });
    }
}

pub fn layout(seq: &Sequence, text_size: impl Fn(&str) -> (f64, f64)) -> Layout {
    let text_size = |s: &str| {
        if s.is_empty() {
            (0.0, 0.0)
        } else {
            text_size(s)
        }
    };
    let n = seq.participants.len();

    let mut top = MARGIN;
    let title = seq.title.as_ref().map(|t| {
        let (_, th) = text_size(t);
        let center_y = top + th / 2.0;
        top += th + 16.0;
        center_y
    });

    // Participant boxes.
    let heads: Vec<(f64, f64)> = seq
        .participants
        .iter()
        .map(|p| {
            let (tw, th) = text_size(&p.label);
            match p.kind {
                ParticipantKind::Participant => (
                    (tw + 2.0 * PARTICIPANT_PADDING.0).max(MIN_PARTICIPANT_SIZE.0),
                    (th + 2.0 * PARTICIPANT_PADDING.1).max(MIN_PARTICIPANT_SIZE.1),
                ),
                ParticipantKind::Actor => (tw.max(40.0) + 8.0, ACTOR_FIGURE_HEIGHT + th + 4.0),
            }
        })
        .collect();
    let index = |id: &str| seq.participant_index(id);

    // Minimum distances between neighbouring column centers.
    let mut gaps: Vec<f64> = (1..n)
        .map(|i| heads[i - 1].0 / 2.0 + heads[i].0 / 2.0 + COLUMN_GAP)
        .collect();
    let mut left_room = heads.first().map_or(0.0, |h| h.0 / 2.0);
    let mut right_room = heads.last().map_or(0.0, |h| h.0 / 2.0);
    for item in &seq.items {
        match item {
            Item::Message(m) => {
                let (Some(a), Some(b)) = (index(&m.from), index(&m.to)) else {
                    continue;
                };
                let (tw, _) = text_size(&m.text);
                if a == b {
                    let need = tw.max(SELF_LOOP_SIZE.0) + LABEL_GAP;
                    match gaps.get_mut(a) {
                        Some(gap) => *gap = gap.max(need),
                        None => right_room = right_room.max(need),
                    }
                } else {
                    let (lo, hi) = (a.min(b), a.max(b));
                    let need = tw + LABEL_GAP;
                    let have: f64 = gaps[lo..hi].iter().sum();
                    if have < need {
                        let extra = (need - have) / (hi - lo) as f64;
                        gaps[lo..hi].iter_mut().for_each(|g| *g += extra);
                    }
                }
            }
            Item::Note(note) => {
                let Some(a) = note.participants.first().and_then(|id| index(id)) else {
                    continue;
                };
                let (tw, _) = text_size(&note.text);
                let need = (tw + 2.0 * NOTE_PADDING.0).max(NOTE_MIN_WIDTH) + 2.0 * NOTE_OFFSET;
                match note.placement {
                    NotePlacement::RightOf => match gaps.get_mut(a) {
                        Some(gap) => *gap = gap.max(need),
                        None => right_room = right_room.max(need),
                    },
                    NotePlacement::LeftOf => match a.checked_sub(1).and_then(|i| gaps.get_mut(i)) {
                        Some(gap) => *gap = gap.max(need),
                        None => left_room = left_room.max(need),
                    },
                    NotePlacement::Over => {}
                }
            }
            _ => {}
        }
    }
    let mut centers = Vec::with_capacity(n);
    let mut x = MARGIN + left_room;
    for i in 0..n {
        centers.push(x);
        if let Some(gap) = gaps.get(i) {
            x += gap;
        }
    }

    let header_height = heads.iter().map(|h| h.1).fold(0.0, f64::max);
    let lifeline_top = top + header_height;

    // Rows.
    let mut rows = Vec::with_capacity(seq.items.len());
    let mut activations = Vec::new();
    let mut active: Vec<Vec<f64>> = vec![Vec::new(); n];
    let mut frames = Vec::new();
    let mut open: Vec<OpenFrame> = Vec::new();
    let mut number = 0;
    let mut y = lifeline_top + ROW_GAP;

    let edge = |depth: usize| {
        if depth == 0 {
            0.0
        } else {
            ACTIVATION_WIDTH / 2.0 + (depth - 1) as f64 * ACTIVATION_NESTING
        }
    };
    let close_bar =
        |active: &mut Vec<Vec<f64>>, activations: &mut Vec<ActivationBar>, p: usize, y: f64| {
            let depth = active[p].len();
            if let Some(start) = active[p].pop() {
                activations.push(ActivationBar {
                    participant: p,
                    rect: Rect {
                        x: centers[p] - ACTIVATION_WIDTH / 2.0
                            + (depth - 1) as f64 * ACTIVATION_NESTING,
                        y: start,
                        w: ACTIVATION_WIDTH,
                        h: (y - start).max(4.0),
                    },
                });
            }
        };

    for (i, item) in seq.items.iter().enumerate() {
        let row_top = y;
        let row = match item {
            Item::Message(m) => {
                let (Some(a), Some(b)) = (index(&m.from), index(&m.to)) else {
                    rows.push(Row {
                        top: y,
                        bottom: y,
                        shape: RowShape::Marker { y },
                    });
                    continue;
                };
                let (tw, th) = text_size(&m.text);
                let line_y = y + th + 8.0;
                if seq.autonumber {
                    number += 1;
                }
                let (ca, cb) = (centers[a], centers[b]);
                let x1;
                let x2;
                let label;
                let bottom;
                if a == b {
                    x1 = ca + edge(active[a].len());
                    x2 = x1;
                    label = Point::new(x1 + 6.0 + tw / 2.0, line_y - 4.0 - th / 2.0);
                    bottom = line_y + SELF_LOOP_SIZE.1 + ROW_GAP;
                    for f in &mut open {
                        f.include(
                            x1.min(label.x - tw / 2.0),
                            (x1 + SELF_LOOP_SIZE.0).max(label.x + tw / 2.0),
                        );
                    }
                } else {
                    let dir = if cb > ca { 1.0 } else { -1.0 };
                    x1 = ca + dir * edge(active[a].len());
                    let receiver_depth = active[b].len() + usize::from(m.activate);
                    x2 = cb - dir * edge(receiver_depth);
                    label = Point::new((ca + cb) / 2.0, line_y - 4.0 - th / 2.0);
                    bottom = line_y + ROW_GAP;
                    for f in &mut open {
                        f.include(ca.min(cb), ca.max(cb));
                    }
                }
                if m.activate {
                    active[b].push(line_y);
                }
                if m.deactivate {
                    close_bar(&mut active, &mut activations, a, line_y);
                }
                Row {
                    top: row_top,
                    bottom,
                    shape: RowShape::Message {
                        y: line_y,
                        x1,
                        x2,
                        self_loop: a == b,
                        number: seq.autonumber.then_some(number),
                        label,
                    },
                }
            }
            Item::Note(note) => {
                let cols: Vec<usize> = note
                    .participants
                    .iter()
                    .filter_map(|id| index(id))
                    .collect();
                let Some(&first) = cols.first() else {
                    rows.push(Row {
                        top: y,
                        bottom: y,
                        shape: RowShape::Marker { y },
                    });
                    continue;
                };
                let (tw, th) = text_size(&note.text);
                let mut w = (tw + 2.0 * NOTE_PADDING.0).max(NOTE_MIN_WIDTH);
                let h = th + 2.0 * NOTE_PADDING.1;
                let c = centers[first];
                let x = match note.placement {
                    NotePlacement::RightOf => c + NOTE_OFFSET + edge(active[first].len()),
                    NotePlacement::LeftOf => c - NOTE_OFFSET - w,
                    NotePlacement::Over => {
                        let lo = cols
                            .iter()
                            .map(|&i| centers[i])
                            .fold(f64::INFINITY, f64::min);
                        let hi = cols
                            .iter()
                            .map(|&i| centers[i])
                            .fold(f64::NEG_INFINITY, f64::max);
                        w = w.max(hi - lo + 2.0 * NOTE_OFFSET + 20.0);
                        (lo + hi) / 2.0 - w / 2.0
                    }
                };
                let rect = Rect {
                    x,
                    y: y + 4.0,
                    w,
                    h,
                };
                for f in &mut open {
                    f.include(rect.x, rect.x + rect.w);
                }
                Row {
                    top: row_top,
                    bottom: rect.y + rect.h + ROW_GAP,
                    shape: RowShape::Note(rect),
                }
            }
            Item::Activate(id) | Item::Deactivate(id) => {
                let marker_y = y + MARKER_ROW / 2.0;
                let x = index(id).map_or(0.0, |p| centers[p]);
                if let Some(p) = index(id) {
                    if matches!(item, Item::Activate(_)) {
                        active[p].push(marker_y);
                    } else {
                        close_bar(&mut active, &mut activations, p, marker_y);
                    }
                }
                Row {
                    top: row_top,
                    bottom: y + MARKER_ROW,
                    shape: RowShape::Activation { y: marker_y, x },
                }
            }
            Item::BlockStart { kind, .. } => {
                open.push(OpenFrame {
                    start: i,
                    kind: *kind,
                    top: y + 2.0,
                    branches: Vec::new(),
                    extent: None,
                    levels: 0,
                });
                Row {
                    top: row_top,
                    bottom: y + BLOCK_HEADER + 8.0,
                    shape: RowShape::Marker { y: y + 2.0 },
                }
            }
            Item::BlockBranch { .. } => {
                if let Some(f) = open.last_mut() {
                    f.branches.push((i, y + 2.0));
                }
                Row {
                    top: row_top,
                    bottom: y + BLOCK_HEADER + 4.0,
                    shape: RowShape::Marker { y: y + 2.0 },
                }
            }
            Item::BlockEnd => {
                let end_y = y + 4.0;
                if let Some(f) = open.pop() {
                    let pad = BLOCK_PADDING + f.levels as f64 * BLOCK_NESTING;
                    let (lo, hi) = f.extent.unwrap_or_else(|| {
                        (
                            centers.first().copied().unwrap_or(MARGIN),
                            centers.last().copied().unwrap_or(MARGIN),
                        )
                    });
                    let rect = Rect {
                        x: lo - pad,
                        y: f.top,
                        w: (hi - lo + 2.0 * pad).max(120.0),
                        h: end_y - f.top,
                    };
                    if let Some(parent) = open.last_mut() {
                        parent.levels = parent.levels.max(f.levels + 1);
                        parent.include(rect.x, rect.x + rect.w);
                    }
                    frames.push(Frame {
                        kind: f.kind,
                        start: f.start,
                        end: i,
                        branches: f.branches,
                        rect,
                    });
                }
                Row {
                    top: row_top,
                    bottom: end_y + ROW_GAP,
                    shape: RowShape::Marker { y: end_y },
                }
            }
        };
        y = row.bottom;
        rows.push(row);
    }

    let lifeline_bottom = y + ROW_GAP / 2.0;
    for p in 0..n {
        while !active[p].is_empty() {
            close_bar(&mut active, &mut activations, p, lifeline_bottom);
        }
    }
    // Outer frames are drawn first so nested ones stay visible.
    frames.sort_by_key(|f| f.start);

    let columns: Vec<Column> = centers
        .iter()
        .zip(&heads)
        .map(|(&c, &(w, h))| Column {
            center: c,
            head: Rect {
                x: c - w / 2.0,
                y: top,
                w,
                h,
            },
            foot: Rect {
                x: c - w / 2.0,
                y: lifeline_bottom,
                w,
                h,
            },
        })
        .collect();

    let mut bounds = Rect {
        x: MARGIN,
        y: MARGIN,
        w: 1.0,
        h: lifeline_bottom - MARGIN,
    };
    for c in &columns {
        bounds = bounds.union(&c.head).union(&c.foot);
    }
    for f in &frames {
        bounds = bounds.union(&f.rect);
    }
    for row in &rows {
        if let RowShape::Note(r) = &row.shape {
            bounds = bounds.union(r);
        }
    }
    let title = title.map(|y| Point::new(bounds.center().x, y));

    Layout {
        title,
        columns,
        rows,
        activations,
        frames,
        lifeline_top,
        lifeline_bottom,
        bounds,
    }
}

impl Layout {
    pub fn hit(&self, p: Point) -> Option<Hit> {
        if let Some(i) = self
            .columns
            .iter()
            .position(|c| c.head.contains(p) || c.foot.contains(p))
        {
            return Some(Hit::Participant(i));
        }
        if let Some(i) = self.item_at(p) {
            return Some(Hit::Item(i));
        }
        self.lifeline_at(p).map(Hit::Lifeline)
    }

    fn item_at(&self, p: Point) -> Option<usize> {
        // Rows first, from the bottom so later rows win.
        for (i, row) in self.rows.iter().enumerate().rev() {
            let hit = match &row.shape {
                RowShape::Message {
                    y,
                    x1,
                    x2,
                    self_loop,
                    label,
                    ..
                } => {
                    let (lo, hi) = if *self_loop {
                        (*x1, x1 + SELF_LOOP_SIZE.0)
                    } else {
                        // Leave the ends to the lifelines so dragging from a
                        // lifeline still starts a new message.
                        (x1.min(*x2) + LIFELINE_HIT, x1.max(*x2) - LIFELINE_HIT)
                    };
                    let on_line = p.x >= lo && p.x <= hi && p.y >= row.top && p.y <= y + 6.0;
                    let on_loop = *self_loop
                        && p.x >= lo
                        && p.x <= hi
                        && p.y >= *y
                        && p.y <= y + SELF_LOOP_SIZE.1;
                    let on_label = (p.x - label.x).abs() < 30.0 && (p.y - label.y).abs() < 10.0;
                    on_line || on_loop || on_label
                }
                RowShape::Note(rect) => rect.contains(p),
                RowShape::Activation { y, x } => {
                    (p.y - y).abs() <= MARKER_ROW / 2.0 && (p.x - x).abs() <= 30.0
                }
                RowShape::Marker { .. } => false,
            };
            if hit {
                return Some(i);
            }
        }
        // Then block edges: the header strip, branch separators and the bottom edge.
        for frame in self.frames.iter().rev() {
            let r = &frame.rect;
            let within_x = p.x >= r.x && p.x <= r.x + r.w;
            if !within_x {
                continue;
            }
            if p.y >= r.y - FRAME_EDGE_HIT && p.y <= r.y + BLOCK_HEADER {
                return Some(frame.start);
            }
            if let Some((i, _)) = frame
                .branches
                .iter()
                .find(|(_, y)| p.y >= y - FRAME_EDGE_HIT && p.y <= y + BLOCK_HEADER)
            {
                return Some(*i);
            }
            if (p.y - (r.y + r.h)).abs() <= FRAME_EDGE_HIT {
                return Some(frame.end);
            }
        }
        None
    }

    pub fn lifeline_at(&self, p: Point) -> Option<usize> {
        if p.y < self.lifeline_top || p.y > self.lifeline_bottom {
            return None;
        }
        self.nearest_column(p.x)
            .filter(|&i| (self.columns[i].center - p.x).abs() <= LIFELINE_HIT)
    }

    pub fn nearest_column(&self, x: f64) -> Option<usize> {
        self.columns
            .iter()
            .enumerate()
            .min_by(|a, b| (a.1.center - x).abs().total_cmp(&(b.1.center - x).abs()))
            .map(|(i, _)| i)
    }

    /// Participant position a column dropped at `x` should take.
    pub fn column_slot(&self, x: f64) -> usize {
        self.columns.iter().filter(|c| c.center < x).count()
    }

    /// Item position a row dropped at `y` should take.
    pub fn row_slot(&self, y: f64) -> usize {
        self.rows
            .iter()
            .position(|r| y < (r.top + r.bottom) / 2.0)
            .unwrap_or(self.rows.len())
    }

    /// Y of the boundary above the item at `slot`.
    pub fn slot_y(&self, slot: usize) -> f64 {
        match self.rows.get(slot) {
            Some(row) => row.top,
            None => self
                .rows
                .last()
                .map_or(self.lifeline_top + ROW_GAP, |r| r.bottom),
        }
    }

    /// Item index of the row covering `y`, if any.
    pub fn row_at(&self, y: f64) -> Option<usize> {
        self.rows.iter().position(|r| y >= r.top && y < r.bottom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sequence::parser::parse;

    fn size(s: &str) -> (f64, f64) {
        (s.len() as f64 * 8.0, 16.0)
    }

    #[test]
    fn columns_follow_participant_order() {
        let seq = parse("sequenceDiagram\n A->>B: hi\n B->>C: yo").unwrap();
        let l = layout(&seq, size);
        assert_eq!(l.columns.len(), 3);
        assert!(l.columns[0].center < l.columns[1].center);
        assert!(l.columns[1].center < l.columns[2].center);
    }

    #[test]
    fn long_labels_widen_columns() {
        let short = layout(&parse("sequenceDiagram\n A->>B: x").unwrap(), size);
        let long = layout(
            &parse("sequenceDiagram\n A->>B: a much much longer message text here").unwrap(),
            size,
        );
        let gap = |l: &Layout| l.columns[1].center - l.columns[0].center;
        assert!(gap(&long) > gap(&short));
        assert!(gap(&long) >= 40.0 * 8.0);
    }

    #[test]
    fn rows_go_down_in_item_order() {
        let seq = parse("sequenceDiagram\n A->>B: 1\n Note right of B: n\n B-->>A: 2").unwrap();
        let l = layout(&seq, size);
        assert_eq!(l.rows.len(), 3);
        assert!(l.rows.windows(2).all(|w| w[0].bottom <= w[1].top + 1e-9));
        assert!(l.lifeline_bottom >= l.rows[2].bottom);
    }

    #[test]
    fn activation_bars_span_their_messages() {
        let seq = parse("sequenceDiagram\n A->>+B: req\n B-->>-A: resp").unwrap();
        let l = layout(&seq, size);
        assert_eq!(l.activations.len(), 1);
        let bar = &l.activations[0];
        assert_eq!(bar.participant, 1);
        let y = |i: usize| match l.rows[i].shape {
            RowShape::Message { y, .. } => y,
            _ => unreachable!(),
        };
        assert_eq!(bar.rect.y, y(0));
        assert_eq!(bar.rect.y + bar.rect.h, y(1));
        // The request ends at the bar's edge, not the lifeline.
        let RowShape::Message { x2, .. } = l.rows[0].shape else {
            unreachable!()
        };
        assert_eq!(x2, l.columns[1].center - ACTIVATION_WIDTH / 2.0);
    }

    #[test]
    fn frames_enclose_their_rows() {
        let seq = parse(
            "sequenceDiagram\n loop forever\n alt x\n A->>B: m\n else\n B->>A: n\n end\n end",
        )
        .unwrap();
        let l = layout(&seq, size);
        assert_eq!(l.frames.len(), 2);
        let (outer, inner) = (&l.frames[0], &l.frames[1]);
        assert_eq!(outer.kind, BlockKind::Loop);
        assert!(outer.rect.x < inner.rect.x);
        assert!(outer.rect.y < inner.rect.y);
        assert!(outer.rect.y + outer.rect.h > inner.rect.y + inner.rect.h);
        assert_eq!(inner.branches.len(), 1);
    }

    #[test]
    fn hit_testing_finds_elements() {
        let seq = parse("sequenceDiagram\n A->>B: hello\n loop\n B->>A: back\n end").unwrap();
        let l = layout(&seq, size);
        assert_eq!(l.hit(l.columns[0].head.center()), Some(Hit::Participant(0)));
        let RowShape::Message { y, label, .. } = l.rows[0].shape else {
            unreachable!()
        };
        assert_eq!(l.hit(label), Some(Hit::Item(0)));
        let mid = (l.columns[0].center + l.columns[1].center) / 2.0;
        assert_eq!(l.hit(Point::new(mid, y)), Some(Hit::Item(0)));
        assert_eq!(
            l.hit(Point::new(l.columns[1].center, y)),
            Some(Hit::Lifeline(1))
        );
        let frame = &l.frames[0];
        assert_eq!(
            l.hit(Point::new(frame.rect.x + 4.0, frame.rect.y + 4.0)),
            Some(Hit::Item(1))
        );
        assert_eq!(
            l.hit(Point::new(frame.rect.x + 4.0, frame.rect.y + frame.rect.h)),
            Some(Hit::Item(3))
        );
    }

    #[test]
    fn slots_map_positions_to_indices() {
        let seq = parse("sequenceDiagram\n A->>B: 1\n A->>B: 2").unwrap();
        let l = layout(&seq, size);
        assert_eq!(l.row_slot(l.rows[0].top), 0);
        assert_eq!(l.row_slot(l.rows[1].bottom - 1.0), 2);
        assert_eq!(l.column_slot(l.columns[0].center - 1.0), 0);
        assert_eq!(l.column_slot(l.columns[1].center + 1.0), 2);
    }
}

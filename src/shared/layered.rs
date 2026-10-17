//! Layered (Sugiyama-style) auto layout for node-link diagrams: nodes are
//! ranked along the flow direction and ordered within ranks to reduce crossings.

use crate::shared::geometry::Point;

const ORDERING_SWEEPS: usize = 4;
/// Top-left margin of a laid out diagram.
const MARGIN: f64 = 40.0;

/// Gaps between consecutive ranks and between nodes within a rank.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Spacing {
    pub rank: f64,
    pub node: f64,
}

impl Default for Spacing {
    fn default() -> Self {
        Self {
            rank: 70.0,
            node: 40.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Direction {
    #[default]
    TopBottom,
    BottomTop,
    LeftRight,
    RightLeft,
}

impl Direction {
    pub const ALL: [Direction; 4] = [
        Direction::TopBottom,
        Direction::BottomTop,
        Direction::LeftRight,
        Direction::RightLeft,
    ];

    pub fn keyword(self) -> &'static str {
        match self {
            Direction::TopBottom => "TB",
            Direction::BottomTop => "BT",
            Direction::LeftRight => "LR",
            Direction::RightLeft => "RL",
        }
    }

    pub fn from_keyword(s: &str) -> Option<Self> {
        match s {
            "TB" | "TD" => Some(Direction::TopBottom),
            "BT" => Some(Direction::BottomTop),
            "LR" => Some(Direction::LeftRight),
            "RL" => Some(Direction::RightLeft),
            _ => None,
        }
    }
}

/// Fills every `None` in `positions` (node centers). When some nodes already
/// have positions, the new ones are offset to stay consistent with them.
/// `sizes` and `positions` are indexed by node; `edges` are node index pairs.
pub fn fill_missing(
    positions: &mut [Option<Point>],
    sizes: &[(f64, f64)],
    edges: &[(usize, usize)],
    direction: Direction,
    spacing: Spacing,
) {
    if positions.iter().all(Option::is_some) {
        return;
    }
    let computed = layout(sizes, edges, direction, spacing);
    let placed: Vec<_> = positions
        .iter()
        .zip(&computed)
        .filter_map(|(pos, auto)| pos.map(|pos| (pos.x - auto.x, pos.y - auto.y)))
        .collect();
    let offset = if placed.is_empty() {
        (0.0, 0.0)
    } else {
        let n = placed.len() as f64;
        (
            placed.iter().map(|p| p.0).sum::<f64>() / n,
            placed.iter().map(|p| p.1).sum::<f64>() / n,
        )
    };
    for (pos, auto) in positions.iter_mut().zip(computed) {
        pos.get_or_insert(Point::new(auto.x + offset.0, auto.y + offset.1));
    }
}

/// Node centers for a fresh layout, indexed like `sizes`.
pub fn layout(
    sizes: &[(f64, f64)],
    edges: &[(usize, usize)],
    direction: Direction,
    spacing: Spacing,
) -> Vec<Point> {
    let n = sizes.len();
    if n == 0 {
        return Vec::new();
    }
    let edges: Vec<(usize, usize)> = edges
        .iter()
        .copied()
        .filter(|&(a, b)| a != b && a < n && b < n)
        .collect();
    let edges = remove_cycles(n, &edges);
    let ranks = assign_ranks(n, &edges);
    let layers = order_layers(n, &edges, &ranks);

    let horizontal = matches!(direction, Direction::LeftRight | Direction::RightLeft);
    // (cross-axis, main-axis) extent of each node
    let extents: Vec<(f64, f64)> = sizes
        .iter()
        .map(|&(w, h)| if horizontal { (h, w) } else { (w, h) })
        .collect();

    let mut centers = vec![(0.0, 0.0); n];
    let mut main = 0.0;
    for layer in &layers {
        let depth = layer.iter().map(|&v| extents[v].1).fold(0.0, f64::max);
        let total: f64 = layer.iter().map(|&v| extents[v].0).sum::<f64>()
            + spacing.node * (layer.len().saturating_sub(1)) as f64;
        let mut cross = -total / 2.0;
        for &v in layer {
            centers[v] = (cross + extents[v].0 / 2.0, main + depth / 2.0);
            cross += extents[v].0 + spacing.node;
        }
        main += depth + spacing.rank;
    }

    let mut points: Vec<Point> = centers
        .into_iter()
        .map(|(cross, main)| match direction {
            Direction::TopBottom => Point::new(cross, main),
            Direction::BottomTop => Point::new(cross, -main),
            Direction::LeftRight => Point::new(main, cross),
            Direction::RightLeft => Point::new(-main, cross),
        })
        .collect();

    let min_x = points
        .iter()
        .zip(sizes)
        .map(|(p, s)| p.x - s.0 / 2.0)
        .fold(f64::INFINITY, f64::min);
    let min_y = points
        .iter()
        .zip(sizes)
        .map(|(p, s)| p.y - s.1 / 2.0)
        .fold(f64::INFINITY, f64::min);
    for p in &mut points {
        p.x += MARGIN - min_x;
        p.y += MARGIN - min_y;
    }
    points
}

/// Drops edges that close a cycle (found by DFS in node order) so ranks can be
/// assigned on a DAG.
fn remove_cycles(n: usize, edges: &[(usize, usize)]) -> Vec<(usize, usize)> {
    #[derive(Clone, Copy, PartialEq)]
    enum State {
        New,
        Active,
        Done,
    }
    let mut out_edges = vec![Vec::new(); n];
    for (i, &(a, b)) in edges.iter().enumerate() {
        out_edges[a].push((b, i));
    }
    let mut state = vec![State::New; n];
    let mut back = vec![false; edges.len()];
    for root in 0..n {
        if state[root] != State::New {
            continue;
        }
        let mut stack = vec![(root, 0usize)];
        state[root] = State::Active;
        while let Some(top) = stack.last_mut() {
            let (v, next) = *top;
            top.1 += 1;
            if let Some(&(w, edge)) = out_edges[v].get(next) {
                match state[w] {
                    State::Active => back[edge] = true,
                    State::New => {
                        state[w] = State::Active;
                        stack.push((w, 0));
                    }
                    State::Done => {}
                }
            } else {
                state[v] = State::Done;
                stack.pop();
            }
        }
    }
    edges
        .iter()
        .zip(back)
        .filter(|(_, back)| !back)
        .map(|(&e, _)| e)
        .collect()
}

/// Longest-path ranking: every node sits one rank below its deepest predecessor.
fn assign_ranks(n: usize, edges: &[(usize, usize)]) -> Vec<usize> {
    let mut indegree = vec![0; n];
    let mut out_edges = vec![Vec::new(); n];
    for &(a, b) in edges {
        indegree[b] += 1;
        out_edges[a].push(b);
    }
    let mut ranks = vec![0; n];
    let mut queue: Vec<usize> = (0..n).filter(|&v| indegree[v] == 0).collect();
    while let Some(v) = queue.pop() {
        for &w in &out_edges[v] {
            ranks[w] = ranks[w].max(ranks[v] + 1);
            indegree[w] -= 1;
            if indegree[w] == 0 {
                queue.push(w);
            }
        }
    }
    ranks
}

/// Groups nodes by rank and reduces crossings with barycenter sweeps.
fn order_layers(n: usize, edges: &[(usize, usize)], ranks: &[usize]) -> Vec<Vec<usize>> {
    let depth = ranks.iter().max().map_or(0, |r| r + 1);
    let mut layers = vec![Vec::new(); depth];
    for v in 0..n {
        layers[ranks[v]].push(v);
    }
    let mut preds = vec![Vec::new(); n];
    let mut succs = vec![Vec::new(); n];
    for &(a, b) in edges {
        preds[b].push(a);
        succs[a].push(b);
    }

    let mut position = vec![0.0; n];
    let index_positions = |layers: &Vec<Vec<usize>>, position: &mut Vec<f64>| {
        for layer in layers {
            for (i, &v) in layer.iter().enumerate() {
                position[v] = i as f64;
            }
        }
    };
    index_positions(&layers, &mut position);

    for sweep in 0..ORDERING_SWEEPS {
        let downward = sweep % 2 == 0;
        let order: Vec<usize> = if downward {
            (1..depth).collect()
        } else {
            (0..depth.saturating_sub(1)).rev().collect()
        };
        for r in order {
            let neighbours = if downward { &preds } else { &succs };
            let barycenter = |v: usize| {
                let ns = &neighbours[v];
                if ns.is_empty() {
                    position[v]
                } else {
                    ns.iter().map(|&u| position[u]).sum::<f64>() / ns.len() as f64
                }
            };
            let mut keyed: Vec<(f64, usize)> =
                layers[r].iter().map(|&v| (barycenter(v), v)).collect();
            keyed.sort_by(|a, b| a.0.total_cmp(&b.0));
            layers[r] = keyed.into_iter().map(|(_, v)| v).collect();
            index_positions(&layers, &mut position);
        }
    }
    layers
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIZE: (f64, f64) = (80.0, 40.0);

    #[test]
    fn ranks_follow_edges_in_each_direction() {
        let sizes = [SIZE; 2];
        let p = layout(&sizes, &[(0, 1)], Direction::TopBottom, Spacing::default());
        assert!(p[1].y > p[0].y && p[1].x == p[0].x);
        let p = layout(&sizes, &[(0, 1)], Direction::BottomTop, Spacing::default());
        assert!(p[1].y < p[0].y);
        let p = layout(&sizes, &[(0, 1)], Direction::LeftRight, Spacing::default());
        assert!(p[1].x > p[0].x && p[1].y == p[0].y);
        let p = layout(&sizes, &[(0, 1)], Direction::RightLeft, Spacing::default());
        assert!(p[1].x < p[0].x);
    }

    #[test]
    fn ignores_cycles_self_loops_and_bad_indices() {
        let sizes = [SIZE; 3];
        let p = layout(
            &sizes,
            &[(0, 1), (1, 2), (2, 0), (1, 1), (0, 9)],
            Direction::TopBottom,
            Spacing::default(),
        );
        assert!(p[0].y < p[1].y && p[1].y < p[2].y);
    }

    #[test]
    fn keeps_layout_inside_margin() {
        let sizes = [SIZE; 4];
        for p in layout(
            &sizes,
            &[(0, 1), (0, 2), (0, 3)],
            Direction::RightLeft,
            Spacing::default(),
        ) {
            assert!(p.x - SIZE.0 / 2.0 >= MARGIN - 1e-9 && p.y - SIZE.1 / 2.0 >= MARGIN - 1e-9);
        }
    }

    #[test]
    fn fills_only_missing_positions_next_to_placed_ones() {
        let mut positions = [Some(Point::new(500.0, 500.0)), None];
        fill_missing(
            &mut positions,
            &[SIZE; 2],
            &[(0, 1)],
            Direction::TopBottom,
            Spacing::default(),
        );
        assert_eq!(positions[0], Some(Point::new(500.0, 500.0)));
        let b = positions[1].unwrap();
        assert_eq!(b.x, 500.0);
        assert!(b.y > 500.0);
    }

    #[test]
    fn parses_direction_keywords() {
        for d in Direction::ALL {
            assert_eq!(Direction::from_keyword(d.keyword()), Some(d));
        }
        assert_eq!(Direction::from_keyword("TD"), Some(Direction::TopBottom));
    }
}

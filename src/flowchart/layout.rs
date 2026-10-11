//! Layered (Sugiyama-style) auto layout for flowcharts.

use std::collections::HashMap;

use super::model::{Direction, Flowchart, Node, Point};

const RANK_GAP: f64 = 70.0;
const NODE_GAP: f64 = 40.0;
const ORDERING_SWEEPS: usize = 4;
/// Top-left margin of a laid out diagram.
const MARGIN: f64 = 40.0;

/// Assigns a position to every node without one. When some nodes already have
/// positions, the new ones are offset to stay consistent with them.
pub fn layout_missing(chart: &mut Flowchart, size_of: impl Fn(&Node) -> (f64, f64)) {
    if chart.nodes.iter().all(|n| n.pos.is_some()) {
        return;
    }
    let computed = compute(chart, &size_of);
    let placed: Vec<_> = chart
        .nodes
        .iter()
        .zip(&computed)
        .filter_map(|(node, auto)| node.pos.map(|pos| (pos.x - auto.x, pos.y - auto.y)))
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
    for (node, auto) in chart.nodes.iter_mut().zip(computed) {
        node.pos
            .get_or_insert(Point::new(auto.x + offset.0, auto.y + offset.1));
    }
}

/// Discards every stored position and lays the whole chart out again.
pub fn layout_all(chart: &mut Flowchart, size_of: impl Fn(&Node) -> (f64, f64)) {
    for node in &mut chart.nodes {
        node.pos = None;
    }
    layout_missing(chart, size_of);
}

/// Computes node centers, indexed like `chart.nodes`.
fn compute(chart: &Flowchart, size_of: &impl Fn(&Node) -> (f64, f64)) -> Vec<Point> {
    let n = chart.nodes.len();
    if n == 0 {
        return Vec::new();
    }
    let index: HashMap<&str, usize> = chart
        .nodes
        .iter()
        .enumerate()
        .map(|(i, node)| (node.id.as_str(), i))
        .collect();
    let edges: Vec<(usize, usize)> = chart
        .edges
        .iter()
        .filter_map(|e| Some((*index.get(e.from.as_str())?, *index.get(e.to.as_str())?)))
        .filter(|(a, b)| a != b)
        .collect();
    let edges = remove_cycles(n, &edges);
    let ranks = assign_ranks(n, &edges);
    let layers = order_layers(n, &edges, &ranks);

    let horizontal = matches!(chart.direction, Direction::LeftRight | Direction::RightLeft);
    // (cross-axis, main-axis) extent of each node
    let extents: Vec<(f64, f64)> = chart
        .nodes
        .iter()
        .map(|node| {
            let (w, h) = size_of(node);
            if horizontal { (h, w) } else { (w, h) }
        })
        .collect();

    let mut centers = vec![(0.0, 0.0); n];
    let mut main = 0.0;
    for layer in &layers {
        let depth = layer.iter().map(|&v| extents[v].1).fold(0.0, f64::max);
        let total: f64 = layer.iter().map(|&v| extents[v].0).sum::<f64>()
            + NODE_GAP * (layer.len().saturating_sub(1)) as f64;
        let mut cross = -total / 2.0;
        for &v in layer {
            centers[v] = (cross + extents[v].0 / 2.0, main + depth / 2.0);
            cross += extents[v].0 + NODE_GAP;
        }
        main += depth + RANK_GAP;
    }

    let mut points: Vec<Point> = centers
        .into_iter()
        .map(|(cross, main)| match chart.direction {
            Direction::TopBottom => Point::new(cross, main),
            Direction::BottomTop => Point::new(cross, -main),
            Direction::LeftRight => Point::new(main, cross),
            Direction::RightLeft => Point::new(-main, cross),
        })
        .collect();

    let min_x = chart
        .nodes
        .iter()
        .zip(&points)
        .map(|(node, p)| p.x - size_of(node).0 / 2.0)
        .fold(f64::INFINITY, f64::min);
    let min_y = chart
        .nodes
        .iter()
        .zip(&points)
        .map(|(node, p)| p.y - size_of(node).1 / 2.0)
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
    use crate::flowchart::parser::parse;

    fn size(_: &Node) -> (f64, f64) {
        (80.0, 40.0)
    }

    fn pos(chart: &Flowchart, id: &str) -> Point {
        chart.node(id).unwrap().pos.unwrap()
    }

    #[test]
    fn top_bottom_places_successors_below() {
        let mut chart = parse("flowchart TD\n A --> B --> C\n A --> D").unwrap();
        layout_all(&mut chart, size);
        assert!(pos(&chart, "B").y > pos(&chart, "A").y);
        assert!(pos(&chart, "C").y > pos(&chart, "B").y);
        assert_eq!(pos(&chart, "B").y, pos(&chart, "D").y);
        assert_ne!(pos(&chart, "B").x, pos(&chart, "D").x);
    }

    #[test]
    fn left_right_places_successors_to_the_right() {
        let mut chart = parse("flowchart LR\n A --> B").unwrap();
        layout_all(&mut chart, size);
        assert!(pos(&chart, "B").x > pos(&chart, "A").x);
        assert_eq!(pos(&chart, "B").y, pos(&chart, "A").y);
    }

    #[test]
    fn bottom_top_places_successors_above() {
        let mut chart = parse("flowchart BT\n A --> B").unwrap();
        layout_all(&mut chart, size);
        assert!(pos(&chart, "B").y < pos(&chart, "A").y);
    }

    #[test]
    fn handles_cycles_and_self_loops() {
        let mut chart = parse("flowchart TD\n A --> B --> C --> A\n C --> C").unwrap();
        layout_all(&mut chart, size);
        assert!(pos(&chart, "A").y < pos(&chart, "B").y);
        assert!(pos(&chart, "B").y < pos(&chart, "C").y);
    }

    #[test]
    fn layout_is_kept_inside_margin() {
        let mut chart = parse("flowchart RL\n A --> B & C & D").unwrap();
        layout_all(&mut chart, size);
        for node in &chart.nodes {
            let p = node.pos.unwrap();
            assert!(p.x - 40.0 >= MARGIN - 1e-9 && p.y - 20.0 >= MARGIN - 1e-9);
        }
    }

    #[test]
    fn missing_layout_keeps_existing_positions() {
        let mut chart = parse("flowchart TD\n A --> B\n%% @merman pos A 500 500").unwrap();
        layout_missing(&mut chart, size);
        assert_eq!(pos(&chart, "A"), Point::new(500.0, 500.0));
        let b = pos(&chart, "B");
        assert_eq!(b.x, 500.0);
        assert!(b.y > 500.0);
    }
}

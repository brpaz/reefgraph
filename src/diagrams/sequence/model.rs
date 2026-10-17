use crate::shared::ids::{self, RenameError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ParticipantKind {
    #[default]
    Participant,
    Actor,
}

impl ParticipantKind {
    pub const ALL: [ParticipantKind; 2] = [ParticipantKind::Participant, ParticipantKind::Actor];

    pub fn keyword(self) -> &'static str {
        match self {
            ParticipantKind::Participant => "participant",
            ParticipantKind::Actor => "actor",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Participant {
    pub id: String,
    /// Display name, set with `participant Id as Label`.
    pub label: String,
    pub kind: ParticipantKind,
}

impl Participant {
    pub fn new(id: impl Into<String>, kind: ParticipantKind) -> Self {
        let id = id.into();
        Self {
            label: id.clone(),
            id,
            kind,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LineStyle {
    #[default]
    Solid,
    Dotted,
}

impl LineStyle {
    pub const ALL: [LineStyle; 2] = [LineStyle::Solid, LineStyle::Dotted];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ArrowHead {
    None,
    #[default]
    Filled,
    Cross,
    /// Open arrow, used for asynchronous messages.
    Open,
}

impl ArrowHead {
    pub const ALL: [ArrowHead; 4] = [
        ArrowHead::None,
        ArrowHead::Filled,
        ArrowHead::Cross,
        ArrowHead::Open,
    ];
}

/// Mermaid arrow tokens, ordered so that longer tokens are matched first.
pub const ARROWS: [(&str, LineStyle, ArrowHead); 8] = [
    ("-->>", LineStyle::Dotted, ArrowHead::Filled),
    ("--x", LineStyle::Dotted, ArrowHead::Cross),
    ("--)", LineStyle::Dotted, ArrowHead::Open),
    ("-->", LineStyle::Dotted, ArrowHead::None),
    ("->>", LineStyle::Solid, ArrowHead::Filled),
    ("-x", LineStyle::Solid, ArrowHead::Cross),
    ("-)", LineStyle::Solid, ArrowHead::Open),
    ("->", LineStyle::Solid, ArrowHead::None),
];

pub fn arrow_token(line: LineStyle, head: ArrowHead) -> &'static str {
    ARROWS
        .iter()
        .find(|(_, l, h)| *l == line && *h == head)
        .map(|(token, _, _)| *token)
        .expect("every line/head pair has a token")
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Message {
    pub from: String,
    pub to: String,
    pub text: String,
    pub line: LineStyle,
    pub head: ArrowHead,
    /// `+` shorthand: activates the receiver.
    pub activate: bool,
    /// `-` shorthand: deactivates the sender.
    pub deactivate: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NotePlacement {
    LeftOf,
    #[default]
    RightOf,
    Over,
}

impl NotePlacement {
    pub const ALL: [NotePlacement; 3] = [
        NotePlacement::LeftOf,
        NotePlacement::RightOf,
        NotePlacement::Over,
    ];

    pub fn keyword(self) -> &'static str {
        match self {
            NotePlacement::LeftOf => "left of",
            NotePlacement::RightOf => "right of",
            NotePlacement::Over => "over",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Note {
    pub placement: NotePlacement,
    /// One participant, or two for a note spanning them (`over` only).
    pub participants: Vec<String>,
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockKind {
    Loop,
    Alt,
    Opt,
    Par,
    Critical,
    Break,
    /// Background highlight; its label is the color.
    Rect,
}

impl BlockKind {
    pub const ALL: [BlockKind; 7] = [
        BlockKind::Loop,
        BlockKind::Alt,
        BlockKind::Opt,
        BlockKind::Par,
        BlockKind::Critical,
        BlockKind::Break,
        BlockKind::Rect,
    ];

    pub fn keyword(self) -> &'static str {
        match self {
            BlockKind::Loop => "loop",
            BlockKind::Alt => "alt",
            BlockKind::Opt => "opt",
            BlockKind::Par => "par",
            BlockKind::Critical => "critical",
            BlockKind::Break => "break",
            BlockKind::Rect => "rect",
        }
    }

    pub fn from_keyword(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.keyword() == s)
    }

    /// Keyword separating sections of the block, for kinds that have them.
    pub fn branch_keyword(self) -> Option<&'static str> {
        match self {
            BlockKind::Alt => Some("else"),
            BlockKind::Par => Some("and"),
            BlockKind::Critical => Some("option"),
            _ => None,
        }
    }
}

/// One line of the diagram body. Blocks are kept flat as start, branch and
/// end markers so rows can be reordered freely; [`Sequence::validate`] checks
/// that markers stay balanced.
#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    Message(Message),
    Note(Note),
    Activate(String),
    Deactivate(String),
    BlockStart { kind: BlockKind, label: String },
    BlockBranch { label: String },
    BlockEnd,
}

impl Item {
    pub fn is_block_marker(&self) -> bool {
        matches!(
            self,
            Item::BlockStart { .. } | Item::BlockBranch { .. } | Item::BlockEnd
        )
    }

    /// Participants this item refers to.
    pub fn participants(&self) -> Vec<&str> {
        match self {
            Item::Message(m) => vec![m.from.as_str(), m.to.as_str()],
            Item::Note(n) => n.participants.iter().map(String::as_str).collect(),
            Item::Activate(id) | Item::Deactivate(id) => vec![id.as_str()],
            _ => Vec::new(),
        }
    }
}

/// Words that cannot be used as participant ids.
const RESERVED_IDS: &[&str] = &[
    "participant",
    "actor",
    "activate",
    "deactivate",
    "autonumber",
    "title",
    "loop",
    "alt",
    "else",
    "opt",
    "par",
    "and",
    "critical",
    "option",
    "break",
    "rect",
    "end",
    "box",
    "Note",
    "note",
];

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Sequence {
    pub title: Option<String>,
    pub autonumber: bool,
    pub participants: Vec<Participant>,
    pub items: Vec<Item>,
    /// Lines before the `sequenceDiagram` declaration.
    pub preamble: Vec<String>,
    /// Statements kept verbatim (comments, `links`, `properties`…).
    pub passthrough: Vec<String>,
}

impl Sequence {
    pub fn is_valid_id(id: &str) -> bool {
        ids::is_valid_id(id, RESERVED_IDS)
    }

    pub fn participant(&self, id: &str) -> Option<&Participant> {
        self.participants.iter().find(|p| p.id == id)
    }

    pub fn participant_mut(&mut self, id: &str) -> Option<&mut Participant> {
        self.participants.iter_mut().find(|p| p.id == id)
    }

    pub fn participant_index(&self, id: &str) -> Option<usize> {
        self.participants.iter().position(|p| p.id == id)
    }

    /// Returns the participant with `id`, creating it when missing.
    pub fn ensure_participant(&mut self, id: &str) -> &mut Participant {
        let idx = match self.participant_index(id) {
            Some(idx) => idx,
            None => {
                self.participants
                    .push(Participant::new(id, ParticipantKind::default()));
                self.participants.len() - 1
            }
        };
        &mut self.participants[idx]
    }

    pub fn add_participant(&mut self, kind: ParticipantKind, index: usize) -> String {
        let id = ids::next_free_id(|id| self.participant(id).is_some());
        let index = index.min(self.participants.len());
        self.participants
            .insert(index, Participant::new(id.clone(), kind));
        id
    }

    /// Inserts a copy of `participant` at `index` under the next free id,
    /// returning it. A label that just repeats the id follows the new id.
    pub fn paste_participant(&mut self, participant: &Participant, index: usize) -> String {
        let id = ids::next_free_id(|id| self.participant(id).is_some());
        let label = if participant.label == participant.id {
            id.clone()
        } else {
            participant.label.clone()
        };
        let index = index.min(self.participants.len());
        self.participants.insert(
            index,
            Participant {
                id: id.clone(),
                label,
                kind: participant.kind,
            },
        );
        id
    }

    /// Removes the participant and every message, note and activation that
    /// involves it. Notes spanning two participants keep the other one.
    pub fn remove_participant(&mut self, id: &str) {
        self.participants.retain(|p| p.id != id);
        self.items.retain_mut(|item| match item {
            Item::Message(m) => m.from != id && m.to != id,
            Item::Note(n) => {
                n.participants.retain(|p| p != id);
                !n.participants.is_empty()
            }
            Item::Activate(p) | Item::Deactivate(p) => p != id,
            _ => true,
        });
    }

    pub fn rename_participant(&mut self, old: &str, new: &str) -> Result<(), RenameError> {
        if old == new {
            return Ok(());
        }
        if !Self::is_valid_id(new) {
            return Err(RenameError::Invalid);
        }
        if self.participant(new).is_some() {
            return Err(RenameError::Taken);
        }
        if let Some(p) = self.participant_mut(old) {
            if p.label == old {
                p.label = new.to_string();
            }
            p.id = new.to_string();
        }
        let rename = |s: &mut String| {
            if s == old {
                *s = new.to_string();
            }
        };
        for item in &mut self.items {
            match item {
                Item::Message(m) => {
                    rename(&mut m.from);
                    rename(&mut m.to);
                }
                Item::Note(n) => n.participants.iter_mut().for_each(rename),
                Item::Activate(p) | Item::Deactivate(p) => rename(p),
                _ => {}
            }
        }
        Ok(())
    }

    pub fn move_participant(&mut self, from: usize, to: usize) {
        if from < self.participants.len() {
            let p = self.participants.remove(from);
            let to = to.min(self.participants.len());
            self.participants.insert(to, p);
        }
    }

    /// Indices of all markers (start, branches, end) of the block that the
    /// marker at `index` belongs to.
    pub fn block_markers(&self, index: usize) -> Option<Vec<usize>> {
        let start = match self.items.get(index)? {
            Item::BlockStart { .. } => index,
            Item::BlockBranch { .. } | Item::BlockEnd => {
                let mut depth = 0;
                let mut i = index;
                loop {
                    i = i.checked_sub(1)?;
                    match self.items[i] {
                        Item::BlockEnd => depth += 1,
                        Item::BlockStart { .. } if depth == 0 => break i,
                        Item::BlockStart { .. } => depth -= 1,
                        _ => {}
                    }
                }
            }
            _ => return None,
        };
        let mut markers = vec![start];
        let mut depth = 0;
        for (i, item) in self.items.iter().enumerate().skip(start + 1) {
            match item {
                Item::BlockStart { .. } => depth += 1,
                Item::BlockBranch { .. } if depth == 0 => markers.push(i),
                Item::BlockEnd if depth == 0 => {
                    markers.push(i);
                    return Some(markers);
                }
                Item::BlockEnd => depth -= 1,
                _ => {}
            }
        }
        None
    }

    /// Kind of the block the marker at `index` belongs to.
    pub fn block_kind(&self, index: usize) -> Option<BlockKind> {
        match self.items.get(*self.block_markers(index)?.first()?)? {
            Item::BlockStart { kind, .. } => Some(*kind),
            _ => None,
        }
    }

    /// Removes the item. Removing a block start or end removes all of that
    /// block's markers and keeps its contents.
    pub fn remove_item(&mut self, index: usize) {
        let remove = match self.items.get(index) {
            Some(Item::BlockStart { .. } | Item::BlockEnd) => {
                self.block_markers(index).unwrap_or_else(|| vec![index])
            }
            Some(_) => vec![index],
            None => return,
        };
        for i in remove.into_iter().rev() {
            self.items.remove(i);
        }
    }

    /// Wraps the items in `range` (possibly empty) in a new block.
    pub fn insert_block(&mut self, kind: BlockKind, range: std::ops::Range<usize>) {
        let end = range.end.min(self.items.len());
        let start = range.start.min(end);
        self.items.insert(end, Item::BlockEnd);
        self.items.insert(
            start,
            Item::BlockStart {
                kind,
                label: String::new(),
            },
        );
    }

    /// Moves the item at `from` so it ends up before the item currently at
    /// `slot` (`items.len()` moves it to the end). Fails, leaving the diagram
    /// unchanged, when the move would unbalance a block.
    pub fn move_item(&mut self, from: usize, slot: usize) -> Result<(), String> {
        if from >= self.items.len() || slot > self.items.len() {
            return Err("index out of range".into());
        }
        let before = self.items.clone();
        let item = self.items.remove(from);
        let to = if slot > from { slot - 1 } else { slot };
        self.items.insert(to, item);
        self.validate().inspect_err(|_| self.items = before)
    }

    /// Checks that block markers are balanced and branches sit in blocks that
    /// allow them.
    pub fn validate(&self) -> Result<(), String> {
        let mut stack = Vec::new();
        for item in &self.items {
            match item {
                Item::BlockStart { kind, .. } => stack.push(*kind),
                Item::BlockBranch { .. }
                    if stack.last().and_then(|k| k.branch_keyword()).is_none() =>
                {
                    return Err("branch outside of a block that allows branches".into());
                }
                Item::BlockEnd => {
                    stack.pop().ok_or("`end` without a matching block")?;
                }
                _ => {}
            }
        }
        if stack.is_empty() {
            Ok(())
        } else {
            Err("block is missing its `end`".into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_items(items: Vec<Item>) -> Sequence {
        Sequence {
            items,
            ..Sequence::default()
        }
    }

    fn message(from: &str, to: &str) -> Item {
        Item::Message(Message {
            from: from.into(),
            to: to.into(),
            ..Message::default()
        })
    }

    fn start(kind: BlockKind) -> Item {
        Item::BlockStart {
            kind,
            label: String::new(),
        }
    }

    fn branch() -> Item {
        Item::BlockBranch {
            label: String::new(),
        }
    }

    #[test]
    fn paste_participant_inserts_copy_under_new_id() {
        let mut seq = Sequence::default();
        seq.add_participant(ParticipantKind::Actor, 0);
        seq.add_participant(ParticipantKind::Participant, 1);
        let mut alice = seq.participants[0].clone();
        alice.label = "Alice".into();

        let id = seq.paste_participant(&alice, 1);
        assert_eq!(id, "C");
        let copy = &seq.participants[1];
        assert_eq!(copy.id, "C");
        assert_eq!(copy.label, "Alice");
        assert_eq!(copy.kind, ParticipantKind::Actor);
    }

    #[test]
    fn arrow_tokens_are_unique() {
        for line in LineStyle::ALL {
            for head in ArrowHead::ALL {
                let token = arrow_token(line, head);
                assert_eq!(ARROWS.iter().filter(|(t, _, _)| *t == token).count(), 1);
            }
        }
    }

    #[test]
    fn remove_participant_cleans_up_items() {
        let mut seq = Sequence::default();
        seq.ensure_participant("A");
        seq.ensure_participant("B");
        seq.ensure_participant("C");
        seq.items = vec![
            message("A", "B"),
            message("B", "C"),
            Item::Note(Note {
                placement: NotePlacement::Over,
                participants: vec!["A".into(), "B".into()],
                text: "n".into(),
            }),
            Item::Activate("B".into()),
        ];
        seq.remove_participant("B");
        assert_eq!(seq.participants.len(), 2);
        assert_eq!(seq.items.len(), 1);
        assert_eq!(seq.items[0].participants(), ["A"]);
    }

    #[test]
    fn rename_updates_references() {
        let mut seq = Sequence::default();
        seq.ensure_participant("A");
        seq.ensure_participant("B");
        seq.items = vec![message("A", "B"), Item::Activate("A".into())];
        seq.rename_participant("A", "Alice").unwrap();
        assert_eq!(seq.participants[0].label, "Alice");
        assert_eq!(seq.items[0].participants(), ["Alice", "B"]);
        assert_eq!(seq.items[1], Item::Activate("Alice".into()));
        assert_eq!(
            seq.rename_participant("B", "Alice"),
            Err(RenameError::Taken)
        );
        assert_eq!(
            seq.rename_participant("B", "loop"),
            Err(RenameError::Invalid)
        );
    }

    #[test]
    fn finds_block_markers_with_nesting() {
        let seq = with_items(vec![
            start(BlockKind::Alt),
            message("A", "B"),
            start(BlockKind::Loop),
            Item::BlockEnd,
            branch(),
            message("B", "A"),
            Item::BlockEnd,
        ]);
        assert_eq!(seq.block_markers(0), Some(vec![0, 4, 6]));
        assert_eq!(seq.block_markers(4), Some(vec![0, 4, 6]));
        assert_eq!(seq.block_markers(3), Some(vec![2, 3]));
        assert_eq!(seq.block_kind(6), Some(BlockKind::Alt));
        assert_eq!(seq.block_markers(1), None);
    }

    #[test]
    fn removing_block_keeps_contents() {
        let mut seq = with_items(vec![
            start(BlockKind::Alt),
            message("A", "B"),
            branch(),
            message("B", "A"),
            Item::BlockEnd,
        ]);
        seq.remove_item(4);
        assert_eq!(seq.items, vec![message("A", "B"), message("B", "A")]);
    }

    #[test]
    fn move_item_rejects_unbalanced_blocks() {
        let mut seq = with_items(vec![
            start(BlockKind::Loop),
            message("A", "B"),
            Item::BlockEnd,
        ]);
        let original = seq.items.clone();
        assert!(seq.move_item(2, 0).is_err());
        assert_eq!(seq.items, original);
        // Moving the message out of the loop is fine.
        seq.move_item(1, 3).unwrap();
        assert_eq!(
            seq.items,
            vec![start(BlockKind::Loop), Item::BlockEnd, message("A", "B")]
        );
    }

    #[test]
    fn validate_checks_branch_placement() {
        let mut seq = with_items(vec![start(BlockKind::Loop), branch(), Item::BlockEnd]);
        assert!(seq.validate().is_err());
        seq.items[0] = start(BlockKind::Par);
        assert!(seq.validate().is_ok());
    }

    #[test]
    fn insert_block_wraps_range() {
        let mut seq = with_items(vec![message("A", "B"), message("B", "A")]);
        seq.insert_block(BlockKind::Opt, 1..2);
        assert_eq!(seq.items[1], start(BlockKind::Opt));
        assert_eq!(seq.items[3], Item::BlockEnd);
        assert!(seq.validate().is_ok());
    }
}

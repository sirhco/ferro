use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Block {
    Paragraph { text: String },
    Heading { level: u8, text: String },
    Quote { text: String, cite: Option<String> },
    Code { lang: Option<String>, code: String },
    Image { media_id: String, alt: Option<String> },
    List { ordered: bool, items: Vec<String> },
    Divider,
}

pub type Document = Vec<Block>;

impl Block {
    pub fn label(&self) -> &'static str {
        match self {
            Block::Paragraph { .. } => "Paragraph",
            Block::Heading { .. } => "Heading",
            Block::Quote { .. } => "Quote",
            Block::Code { .. } => "Code",
            Block::Image { .. } => "Image",
            Block::List { .. } => "List",
            Block::Divider => "Divider",
        }
    }

    pub fn empty(kind: BlockKind) -> Block {
        match kind {
            BlockKind::Paragraph => Block::Paragraph { text: String::new() },
            BlockKind::HeadingH1 => Block::Heading { level: 1, text: String::new() },
            BlockKind::HeadingH2 => Block::Heading { level: 2, text: String::new() },
            BlockKind::HeadingH3 => Block::Heading { level: 3, text: String::new() },
            BlockKind::Quote => Block::Quote { text: String::new(), cite: None },
            BlockKind::Code => Block::Code { lang: None, code: String::new() },
            BlockKind::Image => Block::Image { media_id: String::new(), alt: None },
            BlockKind::List => Block::List { ordered: false, items: vec![String::new()] },
            BlockKind::OrderedList => Block::List { ordered: true, items: vec![String::new()] },
            BlockKind::Divider => Block::Divider,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockKind {
    Paragraph,
    HeadingH1,
    HeadingH2,
    HeadingH3,
    Quote,
    Code,
    Image,
    List,
    OrderedList,
    Divider,
}

impl BlockKind {
    pub fn all() -> &'static [BlockKind] {
        &[
            BlockKind::Paragraph,
            BlockKind::HeadingH1,
            BlockKind::HeadingH2,
            BlockKind::HeadingH3,
            BlockKind::Quote,
            BlockKind::Code,
            BlockKind::Image,
            BlockKind::List,
            BlockKind::OrderedList,
            BlockKind::Divider,
        ]
    }

    pub fn label(self) -> &'static str {
        match self {
            BlockKind::Paragraph => "Paragraph",
            BlockKind::HeadingH1 => "Heading 1",
            BlockKind::HeadingH2 => "Heading 2",
            BlockKind::HeadingH3 => "Heading 3",
            BlockKind::Quote => "Quote",
            BlockKind::Code => "Code",
            BlockKind::Image => "Image",
            BlockKind::List => "Bullet list",
            BlockKind::OrderedList => "Numbered list",
            BlockKind::Divider => "Divider",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paragraph_round_trip_json() {
        let doc: Document = vec![
            Block::Heading { level: 2, text: "Hi".into() },
            Block::Paragraph { text: "World".into() },
            Block::Divider,
        ];
        let s = serde_json::to_string(&doc).unwrap();
        let back: Document = serde_json::from_str(&s).unwrap();
        assert_eq!(doc, back);
    }

    #[test]
    fn list_serializes_items() {
        let b = Block::List { ordered: true, items: vec!["a".into(), "b".into()] };
        let s = serde_json::to_string(&b).unwrap();
        assert!(s.contains("\"ordered\":true"));
        assert!(s.contains("\"items\":[\"a\",\"b\"]"));
    }

    #[test]
    fn empty_factory_picks_right_variant() {
        assert!(matches!(Block::empty(BlockKind::HeadingH2), Block::Heading { level: 2, .. }));
        assert!(matches!(Block::empty(BlockKind::OrderedList), Block::List { ordered: true, .. }));
        assert!(matches!(Block::empty(BlockKind::Divider), Block::Divider));
    }
}

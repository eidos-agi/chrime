//! Page views — many perspectives on one page, zero extra storage.
//!
//! Memory model:
//! - The engine keeps **one** buffer: raw HTML (+ url/title).
//! - A *view* is a pure projection: walk once → filter/truncate → return → drop.
//! - Never cache multiple full node lists for the same page.
//! - Switching views costs CPU (re-walk), not RAM (no second copy of the page).
//!
//! Node-ids stay stable across views (same walk order) so `click` still works
//! after an agent switches from `outline` to `links`.

use std::collections::BTreeMap;

use crate::{DomNode, DomSnapshot};

/// Named projection of the current page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ViewKind {
    /// All semantic nodes (headings, links, fields, text, …).
    #[default]
    Full,
    /// Headings only — page outline / TOC.
    Outline,
    /// Links with href.
    Links,
    /// Form fields.
    Fields,
    /// Anything clickable (links + buttons).
    Clickables,
    /// Body text blocks (p/li), not the full read() blob.
    Text,
    /// Headings + clickables; long text truncated — cheap for agents.
    Compact,
    /// Counts + sizes only; empty nodes — cheapest introspection.
    Meta,
}

impl ViewKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ViewKind::Full => "full",
            ViewKind::Outline => "outline",
            ViewKind::Links => "links",
            ViewKind::Fields => "fields",
            ViewKind::Clickables => "clickables",
            ViewKind::Text => "text",
            ViewKind::Compact => "compact",
            ViewKind::Meta => "meta",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "full" | "semantic" | "all" | "default" => Some(ViewKind::Full),
            "outline" | "headings" | "toc" => Some(ViewKind::Outline),
            "links" | "link" => Some(ViewKind::Links),
            "fields" | "forms" | "inputs" => Some(ViewKind::Fields),
            "clickables" | "actions" | "click" | "acts" => Some(ViewKind::Clickables),
            "text" | "paragraphs" => Some(ViewKind::Text),
            "compact" | "skim" | "summary" => Some(ViewKind::Compact),
            "meta" | "stats" | "counts" => Some(ViewKind::Meta),
            _ => None,
        }
    }

    pub fn all() -> &'static [ViewKind] {
        &[
            ViewKind::Full,
            ViewKind::Outline,
            ViewKind::Links,
            ViewKind::Fields,
            ViewKind::Clickables,
            ViewKind::Text,
            ViewKind::Compact,
            ViewKind::Meta,
        ]
    }

    pub fn label(self) -> &'static str {
        match self {
            ViewKind::Full => "Full",
            ViewKind::Outline => "Outline",
            ViewKind::Links => "Links",
            ViewKind::Fields => "Fields",
            ViewKind::Clickables => "Acts",
            ViewKind::Text => "Text",
            ViewKind::Compact => "Compact",
            ViewKind::Meta => "Meta",
        }
    }
}

const COMPACT_TEXT_MAX: usize = 96;

/// Project a full semantic snapshot into a named view.
/// Consumes the input so the full node list can be dropped after filtering.
pub fn project(mut snap: DomSnapshot, kind: ViewKind, html_bytes: usize) -> DomSnapshot {
    snap.view = kind.as_str().into();
    snap.html_bytes = Some(html_bytes);

    // Role counts always cheap metadata (reused for Meta and optional on others).
    let counts = count_roles(&snap.nodes);
    snap.counts = Some(counts);

    match kind {
        ViewKind::Full => {
            snap.node_count = snap.nodes.len();
            snap
        }
        ViewKind::Outline => {
            snap.nodes
                .retain(|n| n.role == "heading" && !n.text.is_empty());
            snap.node_count = snap.nodes.len();
            snap
        }
        ViewKind::Links => {
            snap.nodes.retain(|n| n.role == "link" && n.href.is_some());
            snap.node_count = snap.nodes.len();
            snap
        }
        ViewKind::Fields => {
            snap.nodes
                .retain(|n| n.role == "field" || n.role == "label");
            snap.node_count = snap.nodes.len();
            snap
        }
        ViewKind::Clickables => {
            snap.nodes.retain(|n| n.clickable);
            snap.node_count = snap.nodes.len();
            snap
        }
        ViewKind::Text => {
            snap.nodes
                .retain(|n| n.role == "text" && !n.text.is_empty());
            snap.node_count = snap.nodes.len();
            snap
        }
        ViewKind::Compact => {
            snap.nodes
                .retain(|n| n.role == "heading" || n.clickable || (n.role == "field"));
            for n in &mut snap.nodes {
                if n.text.chars().count() > COMPACT_TEXT_MAX {
                    n.text = truncate_chars(&n.text, COMPACT_TEXT_MAX);
                }
            }
            snap.node_count = snap.nodes.len();
            snap
        }
        ViewKind::Meta => {
            // Drop all node payloads — only counts + sizes remain.
            snap.nodes.clear();
            snap.nodes.shrink_to_fit();
            snap.node_count = 0;
            snap
        }
    }
}

fn count_roles(nodes: &[DomNode]) -> BTreeMap<String, usize> {
    let mut m = BTreeMap::new();
    for n in nodes {
        *m.entry(n.role.clone()).or_insert(0) += 1;
        if n.clickable {
            *m.entry("clickable".into()).or_insert(0) += 1;
        }
    }
    m
}

fn truncate_chars(s: &str, max: usize) -> String {
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i >= max {
            out.push('…');
            break;
        }
        out.push(ch);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> DomSnapshot {
        DomSnapshot {
            view: "full".into(),
            url: Some("https://ex.com/".into()),
            title: Some("T".into()),
            node_count: 4,
            html_bytes: None,
            counts: None,
            nodes: vec![
                DomNode {
                    node_id: 1,
                    tag: "h1".into(),
                    role: "heading".into(),
                    text: "Hello".into(),
                    href: None,
                    clickable: false,
                },
                DomNode {
                    node_id: 2,
                    tag: "a".into(),
                    role: "link".into(),
                    text: "Go".into(),
                    href: Some("https://ex.com/x".into()),
                    clickable: true,
                },
                DomNode {
                    node_id: 3,
                    tag: "p".into(),
                    role: "text".into(),
                    text: "Body".into(),
                    href: None,
                    clickable: false,
                },
                DomNode {
                    node_id: 4,
                    tag: "input".into(),
                    role: "field".into(),
                    text: String::new(),
                    href: None,
                    clickable: false,
                },
            ],
        }
    }

    #[test]
    fn outline_is_headings_only() {
        let v = project(sample(), ViewKind::Outline, 100);
        assert_eq!(v.view, "outline");
        assert_eq!(v.nodes.len(), 1);
        assert_eq!(v.nodes[0].node_id, 1);
        assert!(v.counts.is_some());
    }

    #[test]
    fn meta_drops_nodes() {
        let v = project(sample(), ViewKind::Meta, 42);
        assert!(v.nodes.is_empty());
        assert_eq!(v.html_bytes, Some(42));
        assert_eq!(v.counts.as_ref().unwrap().get("heading"), Some(&1));
    }

    #[test]
    fn node_ids_stable_across_views() {
        let link = project(sample(), ViewKind::Links, 0);
        assert_eq!(link.nodes[0].node_id, 2);
    }
}

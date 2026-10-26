// D2RUST's own: the market browser's filters.
//! The left column of the browser: the name box, the quality groups (any
//! number of them; none picked means all), one node of the type tree, the
//! three flags (any / yes / no) and the price chips. Together with the
//! sort and the page they make the search ([`Filters::query`]).
//!
//! The type tree is D2RUST's own grouping of the game's item type codes.
//! A node sends its codes (a code matches its descendants), a parent
//! without codes of its own its children's; its count is the sum of the
//! realm's facet counts of those codes ([`Facets::types`], counted at
//! each ancestor).

use std::collections::BTreeSet;

use crate::market::source::{Facets, Group, MarketQuery, MarketSort, PriceMode, Tri};

/// One row of the type tree.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TypeNode {
    /// The word of its UI id (`market.filter.type(helms)`).
    pub key: &'static str,
    pub label: &'static str,
    /// The parent's index in [`TYPE_TREE`], `None` for a top row.
    pub parent: Option<usize>,
    /// The item type codes it sends (empty: its children's).
    pub codes: &'static [&'static str],
}

const fn top(key: &'static str, label: &'static str, codes: &'static [&'static str]) -> TypeNode {
    TypeNode {
        key,
        label,
        parent: None,
        codes,
    }
}

const fn child(
    key: &'static str,
    label: &'static str,
    parent: usize,
    codes: &'static [&'static str],
) -> TypeNode {
    TypeNode {
        key,
        label,
        parent: Some(parent),
        codes,
    }
}

/// The type tree, parents before their children.
pub const TYPE_TREE: &[TypeNode] = &[
    top("armor", "Armor", &["armo"]),
    child("helms", "Helms", 0, &["helm"]),
    child("body", "Body armor", 0, &["tors"]),
    child("shields", "Shields", 0, &["shld"]),
    child("gloves", "Gloves", 0, &["glov"]),
    child("boots", "Boots", 0, &["boot"]),
    child("belts", "Belts", 0, &["belt"]),
    top("weapons", "Weapons", &["weap"]),
    child("axes", "Axes", 7, &["axe"]),
    child("swords", "Swords", 7, &["swor"]),
    child("maces", "Maces and hammers", 7, &["blun"]),
    child("polearms", "Polearms and spears", 7, &["pole", "spea"]),
    child("bows", "Bows and crossbows", 7, &["bow", "xbow"]),
    child("staves", "Staves, wands, orbs", 7, &["staf", "wand", "orb"]),
    child("daggers", "Daggers", 7, &["knif"]),
    child("javelins", "Javelins", 7, &["jave"]),
    child("claws", "Claws", 7, &["h2h"]),
    top("charms", "Charms", &["char"]),
    child("small_charms", "Small charms", 17, &["scha"]),
    child("large_charms", "Large charms", 17, &["mcha"]),
    child("grand_charms", "Grand charms", 17, &["lcha"]),
    top("jewelry", "Jewelry", &[]),
    child("rings", "Rings", 21, &["ring"]),
    child("amulets", "Amulets", 21, &["amul"]),
    child("jewels", "Jewels", 21, &["jewl"]),
    top("runes", "Runes", &["rune"]),
    top("gems", "Gems", &["gem"]),
];

/// The item type codes node `i` sends: its own, or its children's.
pub fn node_codes(i: usize) -> Vec<&'static str> {
    let Some(n) = TYPE_TREE.get(i) else {
        return Vec::new();
    };
    if !n.codes.is_empty() {
        return n.codes.to_vec();
    }
    TYPE_TREE
        .iter()
        .filter(|c| c.parent == Some(i))
        .flat_map(|c| c.codes.iter().copied())
        .collect()
}

/// Node `i`'s count: the realm's counts of its codes added up.
pub fn node_count(counts: &Facets, i: usize) -> u32 {
    node_codes(i)
        .iter()
        .map(|c| counts.types.get(*c).copied().unwrap_or(0))
        .sum()
}

/// Whether node `i` has children.
pub fn has_children(i: usize) -> bool {
    TYPE_TREE.iter().any(|c| c.parent == Some(i))
}

/// The node with UI id word `key`.
pub fn node_of_key(key: &str) -> Option<usize> {
    TYPE_TREE.iter().position(|n| n.key == key)
}

/// The tree rows shown: the top rows, and the children of the open ones.
/// With `find` set, every row whose label (or whose parent's label)
/// holds its words, in tree order.
pub fn shown_rows(open: &BTreeSet<usize>, find: &str) -> Vec<usize> {
    let words: Vec<String> = find.split_whitespace().map(str::to_lowercase).collect();
    let hit = |i: usize| {
        let n = &TYPE_TREE[i];
        let mut hay = n.label.to_lowercase();
        if let Some(p) = n.parent {
            hay.push(' ');
            hay.push_str(&TYPE_TREE[p].label.to_lowercase());
        }
        words.iter().all(|w| hay.contains(w.as_str()))
    };
    (0..TYPE_TREE.len())
        .filter(|&i| {
            if !words.is_empty() {
                return hit(i);
            }
            match TYPE_TREE[i].parent {
                None => true,
                Some(p) => open.contains(&p),
            }
        })
        .collect()
}

/// The three flags of the flag chips.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Flag {
    Ethereal,
    Socketed,
    Identified,
}

impl Flag {
    pub const ALL: [Flag; 3] = [Flag::Ethereal, Flag::Socketed, Flag::Identified];

    pub fn label(self) -> &'static str {
        match self {
            Flag::Ethereal => "Ethereal",
            Flag::Socketed => "Socketed",
            Flag::Identified => "Identified",
        }
    }

    /// The word of its UI id.
    pub fn key(self) -> &'static str {
        match self {
            Flag::Ethereal => "ethereal",
            Flag::Socketed => "socketed",
            Flag::Identified => "identified",
        }
    }

    /// Its facet counts.
    pub fn counts(self, f: &Facets) -> [u32; 2] {
        match self {
            Flag::Ethereal => f.ethereal,
            Flag::Socketed => f.socketed,
            Flag::Identified => f.identified,
        }
    }
}

/// The filters of the left column.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Filters {
    pub text: String,
    pub groups: BTreeSet<Group>,
    /// The picked node of [`TYPE_TREE`].
    pub type_node: Option<usize>,
    pub ethereal: Tri,
    pub socketed: Tri,
    pub identified: Tri,
    pub price: PriceMode,
}

/// The page part of a search.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Paging {
    pub page: u16,
    pub page_size: u8,
    /// Grid view: ordered by group first.
    pub group_first: bool,
    /// A filter changed: the facet counts are asked for too.
    pub facets: bool,
}

impl Filters {
    pub fn flag(&self, f: Flag) -> Tri {
        match f {
            Flag::Ethereal => self.ethereal,
            Flag::Socketed => self.socketed,
            Flag::Identified => self.identified,
        }
    }

    /// A click on flag `f`: any, yes, no, any.
    pub fn cycle(&mut self, f: Flag) {
        let t = match f {
            Flag::Ethereal => &mut self.ethereal,
            Flag::Socketed => &mut self.socketed,
            Flag::Identified => &mut self.identified,
        };
        *t = t.next();
    }

    /// A click on group `g`: in or out of the picked groups.
    pub fn toggle_group(&mut self, g: Group) {
        if !self.groups.remove(&g) {
            self.groups.insert(g);
        }
    }

    /// A click on type node `i`: picks it, or clears a picked one.
    pub fn pick_type(&mut self, i: usize) {
        self.type_node = if self.type_node == Some(i) {
            None
        } else {
            Some(i)
        };
    }

    /// The search of these filters, sorted by `sort`.
    pub fn query(&self, sort: MarketSort, desc: bool, paging: Paging) -> MarketQuery {
        MarketQuery {
            text: self.text.trim().to_owned(),
            groups: self.groups.iter().copied().collect(),
            types: self
                .type_node
                .map(|i| node_codes(i).into_iter().map(str::to_owned).collect())
                .unwrap_or_default(),
            ethereal: self.ethereal,
            socketed: self.socketed,
            identified: self.identified,
            price_mode: self.price,
            sort,
            desc,
            group_first: paging.group_first,
            page: paging.page,
            page_size: paging.page_size,
            facets: paging.facets,
            ..MarketQuery::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parents_send_their_own_or_their_childrens_codes() {
        let jewelry = node_of_key("jewelry").unwrap();
        assert_eq!(node_codes(jewelry), ["ring", "amul", "jewl"]);
        assert_eq!(node_codes(0), ["armo"]);
        assert_eq!(node_codes(node_of_key("runes").unwrap()), ["rune"]);
        // Parents come before their children, keys are unique.
        for (i, n) in TYPE_TREE.iter().enumerate() {
            assert!(n.parent.is_none_or(|p| p < i), "{}", n.key);
            assert_eq!(node_of_key(n.key), Some(i), "{}", n.key);
            assert!(!node_codes(i).is_empty(), "{}", n.key);
            // The contract's type codes are at most 4 bytes.
            assert!(node_codes(i).iter().all(|c| c.len() <= 4), "{}", n.key);
        }
    }

    #[test]
    fn facets_fill_tree_counts() {
        let mut c = Facets::default();
        // Counted at each ancestor, as the realm sends them.
        for (t, n) in [
            ("helm", 5),
            ("tors", 5),
            ("armo", 10),
            ("ring", 2),
            ("amul", 3),
        ] {
            c.types.insert(t.into(), n);
        }
        assert_eq!(node_count(&c, node_of_key("helms").unwrap()), 5);
        assert_eq!(node_count(&c, 0), 10);
        assert_eq!(node_count(&c, node_of_key("jewelry").unwrap()), 5);
        assert_eq!(node_count(&c, node_of_key("gems").unwrap()), 0);
    }

    #[test]
    fn the_tree_shows_open_children_and_finds_types() {
        let closed = shown_rows(&BTreeSet::new(), "");
        assert!(closed.iter().all(|&i| TYPE_TREE[i].parent.is_none()));
        let open = shown_rows(&[0].into(), "");
        assert_eq!(open.len(), closed.len() + 6);
        let found: Vec<&str> = shown_rows(&BTreeSet::new(), "charm")
            .into_iter()
            .map(|i| TYPE_TREE[i].key)
            .collect();
        assert_eq!(
            found,
            ["charms", "small_charms", "large_charms", "grand_charms"]
        );
    }

    #[test]
    fn filters_build_query() {
        let mut f = Filters::default();
        f.cycle(Flag::Socketed);
        assert_eq!(f.socketed, Tri::Yes);
        f.cycle(Flag::Socketed);
        f.cycle(Flag::Socketed);
        assert_eq!(f.socketed, Tri::Any);
        f.cycle(Flag::Ethereal);
        f.toggle_group(Group::Unique);
        f.pick_type(1);
        f.price = PriceMode::RunesOnly;
        f.text = " ashen ".into();
        let paging = Paging {
            page: 2,
            page_size: 50,
            group_first: true,
            facets: false,
        };
        let q = f.query(MarketSort::Price, true, paging);
        assert_eq!(q.groups, [Group::Unique]);
        assert_eq!(q.types, ["helm"]);
        assert_eq!((q.ethereal, q.socketed), (Tri::Yes, Tri::Any));
        assert_eq!(q.price_mode, PriceMode::RunesOnly);
        assert_eq!((q.text.as_str(), q.page, q.page_size), ("ashen", 2, 50));
        assert!(q.group_first && !q.facets && q.desc);
        assert_eq!(q.sort, MarketSort::Price);
        f.pick_type(1);
        assert_eq!(f.type_node, None);
    }
}

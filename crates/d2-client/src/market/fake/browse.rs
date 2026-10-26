// D2RUST's own: the fake market's listings and search.
//! The listings of [`super::FakeSource`] and the search over them, with the
//! contract's rules: every set field must match; the facet counts leave
//! out their own filter; type codes match their descendants and are
//! counted at each ancestor; ties go by the listing ID.

use std::collections::BTreeMap;

use crate::market::source::{
    Facets, Goods, Group, ItemView, ListingMode, ListingRow, MarketQuery, MarketSort, Price,
    PriceMode, ResultsPage, Tri,
};

/// The rune codes the fake prices in, and the stand-in names without game
/// data.
pub const RUNES: [(&str, &str); 8] = [
    ("r15", "Rune 15"),
    ("r20", "Rune 20"),
    ("r23", "Rune 23"),
    ("r24", "Rune 24"),
    ("r25", "Rune 25"),
    ("r26", "Rune 26"),
    ("r29", "Rune 29"),
    ("r31", "Rune 31"),
];

/// The item type parents the fake's type codes have (a code matches and
/// is counted at its ancestors too).
const PARENTS: &[(&str, &str)] = &[
    ("helm", "armo"),
    ("circ", "helm"),
    ("tors", "armo"),
    ("shie", "shld"),
    ("shld", "armo"),
    ("glov", "armo"),
    ("boot", "armo"),
    ("belt", "armo"),
    ("swor", "mele"),
    ("axe", "mele"),
    ("mele", "weap"),
    ("staf", "weap"),
    ("bow", "miss"),
    ("miss", "weap"),
    ("scha", "char"),
    ("lcha", "char"),
];

/// Type code `t` and its ancestors.
pub fn lineage(t: &str) -> Vec<&str> {
    let mut out = vec![t];
    let mut cur = t;
    while let Some((_, p)) = PARENTS.iter().find(|(c, _)| *c == cur) {
        out.push(p);
        cur = p;
    }
    out
}

/// One made-up item: name, base code, base stand-in name, type code,
/// group, level requirement, key stats, sockets, ethereal.
type Template = (
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    Group,
    u8,
    &'static [&'static str],
    u8,
    bool,
);

#[rustfmt::skip]
const ITEMS: &[Template] = &[
    ("Ashen Crown", "uap", "Cap", "helm", Group::Unique, 62,
        &["+2 To All Skills", "+1-148 To Life", "Damage Reduced By 10%"], 0, false),
    ("Grimward Visage", "uhm", "Helm", "helm", Group::Unique, 69,
        &["+2 To Necromancer Skill Levels", "30% Faster Cast Rate"], 1, false),
    ("Duskweave Mantle", "uea", "Hide", "tors", Group::Unique, 58,
        &["+1 To All Skills", "15% Faster Hit Recovery", "All Resistances +20"], 0, false),
    ("Stormcall", "7cr", "Blade", "swor", Group::Unique, 63,
        &["+250% Enhanced Damage", "Adds 1-500 Lightning Damage"], 0, false),
    ("Lantern of Vessa", "amu", "Amulet", "amul", Group::Unique, 67,
        &["+2 To All Skills", "+20% Faster Cast Rate", "All Resistances +30"], 0, false),
    ("Hollowbone Band", "rin", "Ring", "ring", Group::Unique, 29,
        &["+1 To All Skills", "+25 To Mana", "Magic Find 15%"], 0, false),
    ("Emberfall", "8ws", "Staff", "staf", Group::Unique, 72,
        &["+4 To Fire Skills", "+40% Faster Cast Rate"], 2, false),
    ("Thornhide", "uhb", "Greaves", "boot", Group::Unique, 65,
        &["+40% Faster Run/Walk", "+50% Enhanced Defense"], 0, true),
    ("Vessa's Vigil", "ci3", "Circlet", "circ", Group::Set, 65,
        &["+1 To All Skills", "Cold Resist +30%"], 0, false),
    ("Korrin's Grip", "ulg", "Gloves", "glov", Group::Set, 50,
        &["+30% Increased Attack Speed", "5% Life Stolen Per Hit"], 0, false),
    ("Korrin's Coil", "zvb", "Belt", "belt", Group::Set, 60,
        &["+10% Faster Hit Recovery", "+40 To Life"], 0, false),
    ("Oathbreaker", "utp", "Plate", "tors", Group::Runeword, 65,
        &["+2 To All Skills", "+45% Faster Run/Walk", "Teleport charges"], 3, false),
    ("Tidecall", "7wa", "Axe", "axe", Group::Runeword, 63,
        &["+300% Enhanced Damage", "Ignore Target's Defense"], 5, true),
    ("Silence of Ages", "6lw", "Bow", "bow", Group::Runeword, 55,
        &["+2 To All Skills", "+200% Enhanced Damage"], 6, false),
    ("Dread Visor", "uap", "Cap", "helm", Group::Rare, 58,
        &["+2 To Paladin Skill Levels", "20% Faster Hit Recovery", "2 Sockets"], 2, false),
    ("Storm Loop", "rin", "Ring", "ring", Group::Rare, 44,
        &["+10% Faster Cast Rate", "+15 To Strength", "Lightning Resist +28%"], 0, false),
    ("Blood Coil", "amu", "Amulet", "amul", Group::Rare, 50,
        &["+2 To Sorceress Skill Levels", "+60 To Mana"], 0, false),
    ("Raven Fist", "ulg", "Gloves", "glov", Group::Crafted, 61,
        &["+20% Increased Attack Speed", "Life Stolen Per Hit 3%"], 0, false),
    ("Cobalt Ring of Ash", "rin", "Ring", "ring", Group::Magic, 30,
        &["+30 To Mana", "Fire Resist +18%"], 0, false),
    ("Small Charm of Vigor", "cm1", "Small Charm", "scha", Group::Magic, 42,
        &["+20 To Life", "All Resistances +5"], 0, false),
    ("Grand Charm of Tides", "cm3", "Grand Charm", "lcha", Group::Magic, 50,
        &["+1 To Cold Skills", "+40 To Life"], 0, false),
    ("Bright Jewel", "jew", "Jewel", "jewl", Group::Magic, 28,
        &["+15% Increased Attack Speed", "+30% Enhanced Damage"], 0, false),
    ("Shield", "uit", "Shield", "shie", Group::Other, 54,
        &["Defense: 148", "4 Sockets"], 4, false),
    ("Rune", "r24", "Rune 24", "rune", Group::Stackable, 51,
        &["Weapons: 30% Better Chance of Getting Magic Items"], 0, false),
];

/// Seller names, made up.
const SELLERS: [&str; 6] = ["Necrolord", "Varkas", "Ilse", "Sanctus", "Orab", "Kaern"];

/// How many listings the fake holds.
pub const LISTINGS: usize = 120;

/// The realm clock the fake answers with: listings are up to 7 days old,
/// its sales (the price checks') up to 90.
pub const NOW_MS: u64 = 365 * 24 * 3600 * 1000;

const DAY_MS: u64 = 24 * 3600 * 1000;

/// The fake's listings.
pub fn listings(names: &dyn Fn(&str) -> Option<String>) -> Vec<ListingRow> {
    (0..LISTINGS)
        .map(|i| listing(i, &ITEMS[i % ITEMS.len()], names))
        .collect()
}

/// The fake's hash of a price: any change of the price changes it.
pub fn price_hash(p: &Price) -> [u8; 32] {
    // FNV-1a over the price's parts, spread over the 32 bytes.
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mut eat = |b: &[u8]| {
        for &x in b {
            h ^= u64::from(x);
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
    };
    eat(&p.gold.to_le_bytes());
    for (c, n) in &p.stacks {
        eat(c.as_bytes());
        eat(&n.to_le_bytes());
    }
    let mut out = [0u8; 32];
    for (i, chunk) in out.chunks_mut(8).enumerate() {
        chunk.copy_from_slice(&h.rotate_left(i as u32 * 13).to_le_bytes());
    }
    out
}

/// Listing `i` of template `t`.
fn listing(i: usize, t: &Template, names: &dyn Fn(&str) -> Option<String>) -> ListingRow {
    let (name, code, base, item_type, group, req, stats, sockets, ethereal) = *t;
    let base = names(code).unwrap_or_else(|| base.to_owned());
    let name = if matches!(group, Group::Other | Group::Stackable) {
        base.clone()
    } else {
        name.to_owned()
    };
    // Every 7th takes offers only, every 5th takes both.
    let mode = match i % 7 {
        3 => ListingMode::OffersOnly,
        _ if i % 5 == 1 => ListingMode::PriceAndOffers,
        _ => ListingMode::Price,
    };
    let price = mode.has_price().then(|| price_of(i));
    let goods = if group == Group::Stackable {
        Goods::Stack {
            code: code.to_owned(),
            count: 1 + (i % 3) as u32,
        }
    } else {
        Goods::Item {
            item_id: 5000 + i as u32,
            record: Vec::new(),
        }
    };
    let posted_ms = NOW_MS - (i as u64 * 5_011_000) % (7 * DAY_MS);
    ListingRow {
        listing: 1000 + i as u64,
        group,
        goods,
        mode,
        price_hash: price.as_ref().map(price_hash).unwrap_or_default(),
        price,
        seller: SELLERS[(i / 3) % SELLERS.len()].to_owned(),
        seller_sales: (i as u32 * 7) % 60,
        posted_ms,
        expires_ms: posted_ms + 7 * DAY_MS,
        note: if i % 9 == 2 {
            "Fair offers welcome".into()
        } else {
            String::new()
        },
        offers: (i % 4) as u16 * u16::from(mode.takes_offers()),
        mine: false,
        item: ItemView {
            name,
            base,
            code: code.to_owned(),
            item_type: item_type.to_owned(),
            level_req: req,
            sockets,
            ethereal,
            identified: group == Group::Stackable || i % 11 != 4,
            stats: stats.iter().map(|s| (*s).to_owned()).collect(),
        },
    }
}

/// A price of the fake's kinds: gold, one or two runes, or both.
fn price_of(i: usize) -> Price {
    let rune = |k: usize| RUNES[k % RUNES.len()].0.to_owned();
    match i % 4 {
        0 => Price::gold(20_000 + (i as u64 * 7_919) % 480_000),
        1 => Price {
            stacks: vec![(rune(i), 1 + (i % 3) as u32)],
            ..Price::default()
        },
        2 => {
            let mut stacks = vec![(rune(i), 1), (rune(i + 3), 1)];
            stacks.sort();
            stacks.dedup_by(|x, y| x.0 == y.0);
            Price {
                stacks,
                ..Price::default()
            }
        }
        _ => Price {
            gold: 50_000,
            stacks: vec![(rune(i), 1)],
            ..Price::default()
        },
    }
}

fn words(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .collect()
}

/// Every word of the search is the start of a word of the listing's
/// names, base or stats.
fn matches_words(l: &ListingRow, want: &[String]) -> bool {
    if want.is_empty() {
        return true;
    }
    let v = &l.item;
    let have = words(&format!("{} {} {}", v.name, v.base, v.stats.join(" ")));
    want.iter()
        .all(|w| have.iter().any(|h| h.starts_with(w.as_str())))
}

/// The filters a listing must pass, each by itself, so a facet can leave
/// its own out.
struct Checks {
    text: bool,
    group: bool,
    types: bool,
    ethereal: bool,
    socketed: bool,
    identified: bool,
    price: bool,
    rest: bool,
}

impl Checks {
    fn of(l: &ListingRow, q: &MarketQuery, words: &[String]) -> Checks {
        let v = &l.item;
        Checks {
            text: matches_words(l, words),
            group: q.groups.is_empty() || q.groups.contains(&l.group),
            types: q.types.is_empty()
                || lineage(&v.item_type)
                    .iter()
                    .any(|t| q.types.iter().any(|w| w == t)),
            ethereal: q.ethereal.allows(v.ethereal),
            socketed: q.socketed.allows(v.sockets > 0),
            identified: q.identified.allows(v.identified),
            price: q.price_mode.allows(l.mode, l.price.as_ref()),
            rest: (q.level.0..=q.level.1).contains(&v.level_req)
                && (q.sockets.0..=q.sockets.1).contains(&v.sockets)
                && caps_allow(&q.caps, l.price.as_ref()),
        }
    }

    fn all(&self) -> bool {
        self.but(|_| false)
    }

    /// Every check but the one `skip` names.
    fn but(&self, skip: impl Fn(&str) -> bool) -> bool {
        [
            ("text", self.text),
            ("group", self.group),
            ("types", self.types),
            ("ethereal", self.ethereal),
            ("socketed", self.socketed),
            ("identified", self.identified),
            ("price", self.price),
            ("rest", self.rest),
        ]
        .iter()
        .all(|(k, ok)| *ok || skip(k))
    }
}

/// A price cap: the price is only that currency and at most `max`.
fn caps_allow(caps: &[(String, u64)], price: Option<&Price>) -> bool {
    if caps.is_empty() {
        return true;
    }
    let Some(p) = price else { return false };
    caps.iter().any(|(cur, max)| {
        if cur.is_empty() {
            p.gold_only() && p.gold <= *max
        } else {
            p.gold == 0
                && p.stacks.len() == 1
                && p.stacks[0].0 == *cur
                && u64::from(p.stacks[0].1) <= *max
        }
    })
}

/// The facet counts of `q` over `all`.
fn facets(all: &[ListingRow], q: &MarketQuery, words: &[String]) -> Facets {
    let mut f = Facets::default();
    let flag = |pair: &mut [u32; 2], v: bool| pair[usize::from(!v)] += 1;
    for l in all {
        let c = Checks::of(l, q, words);
        let v = &l.item;
        if c.but(|k| k == "group") {
            *f.groups.entry(l.group).or_default() += 1;
        }
        if c.but(|k| k == "types") {
            for t in lineage(&v.item_type) {
                *f.types.entry(t.to_owned()).or_default() += 1;
            }
        }
        if c.but(|k| k == "ethereal") {
            flag(&mut f.ethereal, v.ethereal);
        }
        if c.but(|k| k == "socketed") {
            flag(&mut f.socketed, v.sockets > 0);
        }
        if c.but(|k| k == "identified") {
            flag(&mut f.identified, v.identified);
        }
        if c.but(|k| k == "price") {
            for m in PriceMode::ALL {
                if m.allows(l.mode, l.price.as_ref()) {
                    f.price_modes[usize::from(m.code())] += 1;
                }
            }
        }
    }
    f
}

/// The page `q` asks for.
pub fn search(all: &[ListingRow], q: &MarketQuery) -> ResultsPage {
    let words = words(&q.text);
    let mut hits: Vec<&ListingRow> = all
        .iter()
        .filter(|l| Checks::of(l, q, &words).all())
        .filter(|l| {
            // A price sort lists only the listings priced in its currency.
            q.sort != MarketSort::Price
                || l.price.as_ref().is_some_and(|p| {
                    if q.sort_currency.is_empty() {
                        p.gold > 0
                    } else {
                        p.stacks.iter().any(|(c, _)| *c == q.sort_currency)
                    }
                })
        })
        .collect();
    sort(&mut hits, q);
    let per = usize::from(q.page_size.max(1));
    let total = hits.len() as u32;
    let rows = hits
        .into_iter()
        .skip(usize::from(q.page) * per)
        .take(per)
        .cloned()
        .collect();
    ResultsPage {
        total,
        page: q.page,
        page_size: q.page_size,
        now_ms: NOW_MS,
        rows,
        facets: q.facets.then(|| facets(all, q, &words)),
    }
}

fn sort(hits: &mut [&ListingRow], q: &MarketQuery) {
    let price_of = |l: &ListingRow| -> u64 {
        let Some(p) = &l.price else { return 0 };
        if q.sort_currency.is_empty() {
            p.gold
        } else {
            p.stacks
                .iter()
                .find(|(c, _)| *c == q.sort_currency)
                .map_or(0, |(_, n)| u64::from(*n))
        }
    };
    hits.sort_by(|a, b| {
        let o = match q.sort {
            MarketSort::Posted => a.posted_ms.cmp(&b.posted_ms),
            MarketSort::Name => a.item.name.cmp(&b.item.name),
            MarketSort::Level => a.item.level_req.cmp(&b.item.level_req),
            MarketSort::Price => price_of(a).cmp(&price_of(b)),
            MarketSort::Seller => a.seller.cmp(&b.seller),
            // The fake's items carry no stat values.
            MarketSort::Stat => std::cmp::Ordering::Equal,
        };
        let o = if q.desc { o.reverse() } else { o };
        let g = if q.group_first {
            a.group.cmp(&b.group)
        } else {
            std::cmp::Ordering::Equal
        };
        g.then(o).then(a.listing.cmp(&b.listing))
    });
}

/// The listings per group, for tests.
pub fn group_counts(all: &[ListingRow]) -> BTreeMap<Group, u32> {
    let mut m = BTreeMap::new();
    for l in all {
        *m.entry(l.group).or_default() += 1;
    }
    m
}

/// A flag filter's state the fake's listings show, for tests.
pub fn flag_count(all: &[ListingRow], f: impl Fn(&ListingRow) -> bool, t: Tri) -> u32 {
    all.iter().filter(|l| t.allows(f(l))).count() as u32
}

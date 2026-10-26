//! The item-heavy save corpus for the save checks (plan `multiplayer-v2`
//! N6 follow-up): characters whose items are made by `d2-sim`'s item
//! creation through [`d2s_tool::items::make_item`]:
//! - every enabled unique row and every set row (item level 99);
//! - every complete runeword on up to three bases that take its runes
//!   (runes socketed, the runeword activated as a socket fill does);
//! - magic and rare items of every weapon, armor, ring, amulet, charm and
//!   jewel base at item levels 5 to 99, superior and crafted ones at 99;
//! - before / after pairs of the same characters for the kept-ID rule: the
//!   after save has the same items (made from the same seeds) with some
//!   identified, moved, socketed into a runeword or gone, and new ones.
//!
//! ```text
//! cargo run --release -p d2s-tool --example item_corpus -- \
//!     [--game-dir DIR] [--seed N] [--per-save N] [--pairs N] OUT
//! cargo run --release -p d2-save-checks --example corpus -- OUT
//! ```
//!
//! Writes `OUT/<kind>/<n>.d2s`, the pairs as `OUT/pairs/snap/{0,1}/<n>.d2s`
//! and `OUT/manifest.txt` (the items of each file). The saves are made from
//! the user's own tables and stay local.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use d2_formats::d2s::{self, D2s, ItemEntry};
use d2_sim::items::bitstream::read::read_save_entry;
use d2_sim::items::create::max_sockets_at;
use d2_sim::items::{flag, q, ty};
use d2s_tool::items::{code_str, make_item, ItemOpts, ItemSpec, Owner, Pages, ToolGame};
use d2s_tool::save::{new_save, Edits};
use d2s_tool::tables::Tables;
use d2s_tool::{read_options, round_trip, RoundTrip};

const ILVLS: [i32; 7] = [5, 20, 35, 50, 65, 80, 99];

/// A stored item on page `page` (other storage pages when it is full).
fn item(code: [u8; 4], page: u8, opts: ItemOpts) -> ItemSpec {
    ItemSpec {
        code,
        at: None,
        page,
        qty: None,
        opts,
    }
}

fn opts(quality: u8, ilvl: i32) -> ItemOpts {
    ItemOpts {
        quality: Some(quality),
        ilvl: Some(ilvl),
        ..ItemOpts::default()
    }
}

/// The items to make, by kind; and notes on rows no item is made for.
fn jobs(t: &Tables) -> (BTreeMap<&'static str, Vec<ItemSpec>>, Vec<String>) {
    let it = &t.items;
    let mut out: BTreeMap<&'static str, Vec<ItemSpec>> = BTreeMap::new();
    let mut notes = Vec::new();
    for (row, u) in it.uniques.iter().enumerate() {
        if u.enabled {
            let o = ItemOpts {
                index: Some(row as i32),
                ..opts(q::UNIQUE, 99)
            };
            out.entry("unique").or_default().push(item(u.code, 4, o));
        }
    }
    for (row, s) in it.setitems.iter().enumerate() {
        let o = ItemOpts {
            index: Some(row as i32),
            ..opts(q::SET, 99)
        };
        out.entry("set").or_default().push(item(s.item, 4, o));
    }
    let type_of = |code: &[u8; 4]| {
        it.itemtypes
            .iter()
            .position(|y| &y.code == code)
            .map(|k| k as i16)
    };
    let jewelry: Vec<i16> = [b"ring", b"amul"].into_iter().filter_map(type_of).collect();
    let is = |i: usize, k: u16| it.is_type(i, k as i16);
    for (i, r) in it.items.iter().enumerate() {
        let gear = is(i, ty::WEAP) || is(i, ty::ARMO);
        let small = is(i, ty::CHAR) || is(i, ty::JEWL) || jewelry.iter().any(|&k| it.is_type(i, k));
        if r.spawnable == 0 || r.quest != 0 || !(gear || small) {
            continue;
        }
        for ilvl in ILVLS {
            out.entry("magic")
                .or_default()
                .push(item(r.code, 4, opts(q::MAGIC, ilvl)));
            out.entry("rare")
                .or_default()
                .push(item(r.code, 4, opts(q::RARE, ilvl)));
        }
        if gear {
            out.entry("superior")
                .or_default()
                .push(item(r.code, 4, opts(q::SUPERIOR, 99)));
        }
        out.entry("crafted")
            .or_default()
            .push(item(r.code, 4, opts(q::CRAFTED, 99)));
    }
    for (row, w) in it.runes.iter().enumerate() {
        if w.complete == 0 {
            continue;
        }
        let runes: Vec<[u8; 4]> = w
            .runes
            .iter()
            .take_while(|&&r| r >= 1)
            .filter_map(|&r| it.item(r as usize).map(|x| x.code))
            .collect();
        let n = runes.len();
        let bases: Vec<usize> = (0..it.items.len())
            .filter(|&i| {
                let r = &it.items[i];
                r.hasinv != 0
                    && r.quest == 0
                    && r.spawnable != 0
                    && usize::from(r.invwidth) * usize::from(r.invheight) >= n
                    && max_sockets_at(it, i, 99) >= n as i32
                    && w.itype
                        .iter()
                        .take_while(|&&e| e >= 1)
                        .any(|&e| it.is_type(i, e))
                    && !w
                        .etype
                        .iter()
                        .take_while(|&&e| e >= 1)
                        .any(|&e| it.is_type(i, e))
            })
            .collect();
        if n == 0 || bases.is_empty() {
            notes.push(format!(
                "runes row {row}: no base takes its {n} runes at item level 99"
            ));
            continue;
        }
        let mut picks = vec![bases[0], bases[bases.len() / 2], bases[bases.len() - 1]];
        picks.dedup();
        for (k, b) in picks.into_iter().enumerate() {
            // The middle base superior: a runeword keeps its base's
            // quality row.
            let mut o = opts(if k == 1 { q::SUPERIOR } else { q::NORMAL }, 99);
            for (slot, c) in o.runes.iter_mut().zip(&runes) {
                *slot = Some(*c);
            }
            out.entry("runeword")
                .or_default()
                .push(item(it.items[b].code, 4, o));
        }
    }
    (out, notes)
}

/// A level 99 expansion character, `n` its number (name and class).
fn character(t: &Tables, n: u32) -> Result<(D2s, Owner)> {
    let mut name = String::from("Corpus");
    let mut k = n;
    for _ in 0..5 {
        name.push(char::from(b'a' + (k % 26) as u8));
        k /= 26;
    }
    let class = (n % 7) as u8;
    let e = Edits {
        name: Some(name),
        class: Some(class),
        level: Some(99),
        expansion: true,
        hardcore: Some(false),
        time: Some(1_700_000_000),
        ..Edits::default()
    };
    let save = new_save(&e, t)?;
    let owner = Owner {
        class,
        name: save.header.name,
        level: 99,
        hardcore: false,
        expansion: true,
    };
    Ok((save, owner))
}

/// Makes `spec` on its page, else on another storage page. `None`: no page
/// has room (the game's seed is put back, so the next try draws the same).
fn place(
    t: &Tables,
    game: &mut ToolGame,
    o: &Owner,
    pages: &mut Pages,
    spec: &ItemSpec,
) -> Result<Option<ItemEntry>> {
    let mut order = vec![spec.page];
    for p in [0, 4, 3] {
        if !order.contains(&p) {
            order.push(p);
        }
    }
    for page in order {
        let (seed, uniques) = (game.seed, game.uniques.clone());
        let s = ItemSpec {
            page,
            ..spec.clone()
        };
        match make_item(t, game, o, pages, &s) {
            Ok(e) => return Ok(Some(e)),
            Err(e) if e.to_string().starts_with("no free") => {
                game.seed = seed;
                game.uniques = uniques;
            }
            Err(e) => return Err(e),
        }
    }
    Ok(None)
}

fn show(s: &ItemSpec) -> String {
    let o = &s.opts;
    let mut out = code_str(&s.code);
    if let Some(q) = o.quality {
        let _ = write!(out, "/q={q}");
    }
    if let Some(i) = o.index {
        let _ = write!(out, "/idx={i}");
    }
    if let Some(l) = o.ilvl {
        let _ = write!(out, "/ilvl={l}");
    }
    if let Some(n) = o.sockets {
        let _ = write!(out, "/sock={n}");
    }
    let runes: Vec<String> = o.runes.iter().flatten().map(code_str).collect();
    if !runes.is_empty() {
        let _ = write!(out, "/runes={}", runes.join("+"));
    }
    if o.unidentified {
        out.push_str("/unid");
    }
    out
}

/// An error's text without its numbers and quoted codes (a reason to
/// count).
fn reason(e: &anyhow::Error) -> String {
    let mut out = String::new();
    let mut quoted = false;
    for c in e.to_string().chars() {
        match c {
            '"' => quoted = !quoted,
            _ if quoted => {}
            '0'..='9' => {
                if !out.ends_with('#') {
                    out.push('#');
                }
            }
            _ => out.push(c),
        }
    }
    out
}

fn write_save(t: &Tables, save: &D2s, path: &Path) -> Result<()> {
    let bytes = d2s::write(save, t).map_err(|e| anyhow!("write: {e}"))?;
    match round_trip(&bytes, &read_options(&bytes, None), t)? {
        RoundTrip::Same(_) => {}
        RoundTrip::Differs { offset, .. } => {
            bail!("{}: the rewrite differs at {offset:#x}", path.display())
        }
    }
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d)?;
    }
    std::fs::write(path, bytes).with_context(|| path.display().to_string())
}

fn items_of(save: &mut D2s) -> &mut Vec<ItemEntry> {
    &mut save.body.as_mut().expect("a full save").items
}

#[derive(Default)]
struct Count {
    made: u32,
    saves: u32,
    runewords: u32,
    skipped: BTreeMap<String, u32>,
}

fn main() -> Result<()> {
    let mut game_dir = std::env::var_os("D2_GAME_DIR").map(PathBuf::from);
    let (mut seed, mut per_save, mut pairs) = (1u32, 8usize, 300u32);
    let mut out = None;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        let mut val = |what: &str| args.next().with_context(|| format!("{what} needs a value"));
        match a.as_str() {
            "--game-dir" => game_dir = Some(val("--game-dir")?.into()),
            "--seed" => seed = val("--seed")?.parse()?,
            "--per-save" => per_save = val("--per-save")?.parse()?,
            "--pairs" => pairs = val("--pairs")?.parse()?,
            _ => out = Some(PathBuf::from(a)),
        }
    }
    let out = out
        .context("usage: item_corpus [--game-dir DIR] [--seed N] [--per-save N] [--pairs N] OUT")?;
    let game_dir = game_dir.context("no game directory: --game-dir or D2_GAME_DIR")?;
    let t = Tables::load_dir(&game_dir)?;
    let (jobs, notes) = jobs(&t);
    let mut manifest = String::new();
    let mut counts: BTreeMap<&str, Count> = BTreeMap::new();
    let mut made: BTreeMap<&str, Vec<ItemSpec>> = BTreeMap::new();
    let mut next_char = 0u32;

    for (&kind, specs) in &jobs {
        let c = counts.entry(kind).or_default();
        let mut queue = specs.iter().peekable();
        while queue.peek().is_some() {
            let id = next_char;
            next_char += 1;
            let (mut save, owner) = character(&t, id)?;
            let mut game = ToolGame::new(seed.wrapping_add(id), true);
            let mut pages = Pages::new();
            let mut held = Vec::new();
            while held.len() < per_save {
                let Some(&s) = queue.peek() else {
                    break;
                };
                match place(&t, &mut game, &owner, &mut pages, s) {
                    Ok(Some(e)) => {
                        if read_save_entry(&e.bytes, &t.items)
                            .is_ok_and(|r| r.item.item.flags & flag::RUNEWORD != 0)
                        {
                            c.runewords += 1;
                        }
                        items_of(&mut save).push(e);
                        held.push(show(s));
                        made.entry(kind).or_default().push(s.clone());
                        c.made += 1;
                        queue.next();
                    }
                    Ok(None) if held.is_empty() => {
                        *c.skipped.entry("fits no page".into()).or_default() += 1;
                        queue.next();
                    }
                    Ok(None) => break,
                    Err(e) => {
                        *c.skipped.entry(reason(&e)).or_default() += 1;
                        queue.next();
                    }
                }
            }
            if !held.is_empty() {
                let path = out.join(kind).join(format!("{:05}.d2s", c.saves));
                write_save(&t, &save, &path)?;
                let _ = writeln!(manifest, "{}: {}", path.display(), held.join(" "));
                c.saves += 1;
            }
        }
    }

    // The pairs: kept items (stash), unidentified ones and an empty
    // socketed base (inventory) before; after, two kept items moved to the
    // inventory, the others identified, the base filled with its runes, the
    // first item gone and two new ones (another seed).
    let pool = |kind: &str| made.get(kind).map_or(&[][..], Vec::as_slice);
    let kept_pool: Vec<&ItemSpec> = ["unique", "set", "superior", "magic", "rare"]
        .into_iter()
        .flat_map(pool)
        .collect();
    let unid_pool: Vec<&ItemSpec> = ["rare", "magic"].into_iter().flat_map(pool).collect();
    let rw_pool = pool("runeword");
    let (mut pairs_made, mut pairs_skipped) = (0u32, BTreeMap::<String, u32>::new());
    if kept_pool.is_empty() || unid_pool.is_empty() || rw_pool.is_empty() {
        bail!("no items made to pair");
    }
    for p in 0..pairs {
        let pick = |pool: &[&ItemSpec], k: u32| {
            ItemSpec::clone(pool[(p as usize * 7919 + k as usize * 104_729) % pool.len()])
        };
        let kept: Vec<ItemSpec> = (0..6)
            .map(|k| ItemSpec {
                page: 4,
                ..pick(&kept_pool, k)
            })
            .collect();
        let unid: Vec<ItemSpec> = (0..2)
            .map(|k| {
                let mut s = ItemSpec {
                    page: 0,
                    ..pick(&unid_pool, 10 + k)
                };
                s.opts.unidentified = true;
                s
            })
            .collect();
        let rw = ItemSpec {
            page: 0,
            ..rw_pool[p as usize % rw_pool.len()].clone()
        };
        let mut base = rw.clone();
        base.opts.sockets = Some(rw.opts.runes.iter().flatten().count() as i32);
        base.opts.runes = Default::default();
        let before: Vec<ItemSpec> = kept.iter().chain(&unid).cloned().chain([base]).collect();
        let after: Vec<ItemSpec> = kept
            .iter()
            .enumerate()
            .map(|(k, s)| ItemSpec {
                page: if (1..=2).contains(&k) { 0 } else { s.page },
                ..s.clone()
            })
            .chain(unid.iter().map(|s| {
                let mut s = s.clone();
                s.opts.unidentified = false;
                s
            }))
            .chain([rw])
            .collect();
        let new_items: Vec<ItemSpec> = (0..2)
            .map(|k| ItemSpec {
                page: 0,
                ..pick(&unid_pool, 20 + k)
            })
            .collect();
        let id = next_char;
        next_char += 1;
        let mut texts = Vec::new();
        let result = (|| -> Result<(D2s, D2s)> {
            let mut saves = Vec::new();
            for (n, specs) in [&before, &after].into_iter().enumerate() {
                let (mut save, owner) = character(&t, id)?;
                let mut game = ToolGame::new(seed.wrapping_add(id), true);
                let mut pages = Pages::new();
                for s in specs {
                    let e = place(&t, &mut game, &owner, &mut pages, s)?.context("fits no page")?;
                    items_of(&mut save).push(e);
                }
                let mut held: Vec<String> = specs.iter().map(show).collect();
                if n == 1 {
                    items_of(&mut save).remove(0);
                    held.remove(0);
                    let mut found = ToolGame::new(seed.wrapping_add(id) ^ 0x5EED_0000, true);
                    for s in &new_items {
                        let e = place(&t, &mut found, &owner, &mut pages, s)?
                            .context("fits no page")?;
                        items_of(&mut save).push(e);
                        held.push(format!("new:{}", show(s)));
                    }
                }
                texts.push(held.join(" "));
                saves.push(save);
            }
            let after = saves.pop().expect("two saves");
            Ok((saves.pop().expect("two saves"), after))
        })();
        match result {
            Ok((b, a)) => {
                for (n, (save, text)) in [(b, &texts[0]), (a, &texts[1])].into_iter().enumerate() {
                    let path = out
                        .join("pairs")
                        .join("snap")
                        .join(n.to_string())
                        .join(format!("{p:05}.d2s"));
                    write_save(&t, &save, &path)?;
                    let _ = writeln!(manifest, "{}: {text}", path.display());
                }
                pairs_made += 1;
            }
            Err(e) => *pairs_skipped.entry(reason(&e)).or_default() += 1,
        }
    }
    std::fs::write(out.join("manifest.txt"), manifest)?;

    for (kind, c) in &counts {
        println!(
            "{kind}: {} items in {} saves{}",
            c.made,
            c.saves,
            if *kind == "runeword" {
                format!(", {} with the runeword active", c.runewords)
            } else {
                String::new()
            }
        );
        for (why, n) in &c.skipped {
            println!("  skipped {n}: {why}");
        }
    }
    println!("pairs: {pairs_made} (before and after saves)");
    for (why, n) in &pairs_skipped {
        println!("  pair skipped {n}: {why}");
    }
    for n in &notes {
        println!("note: {n}");
    }
    Ok(())
}

//! Tests of the shared stash stores against a fake game whose character
//! lives in a real file, so a crash (a panic, then a fresh open, as a new
//! process would) can be checked for lost or doubled goods.

mod crash;
mod local;
mod online;
mod settings;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::local::{Arrive, GameError, Lift, Lifted, LocalGame};
use crate::*;

pub(crate) const MOD: &str = "d2rust";

/// Items the fake character's inventory holds at most.
pub(crate) const INV_ITEMS: usize = 40;
/// The fake character's gold limit.
pub(crate) const GOLD_MAX: u64 = 990_000;
/// Keys stack up to 12 in one item.
const KEY_STACK: u32 = 12;

pub(crate) fn catalog() -> Catalog {
    Catalog::base(|c| Some(format!("{c} name"))).with_extra(&[Stackable::new("cur1", "Mod Orb")])
}

pub(crate) fn key(mod_id: &str, hardcore: bool) -> EconomyKey {
    EconomyKey {
        hardcore,
        ..EconomyKey::new(mod_id)
    }
}

/// A fresh folder for one test.
pub(crate) fn folder(test: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join("d2-shared-stash-tests")
        .join(format!("{test}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// An item on the fake character.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FakeItem {
    pub unit: u32,
    pub code: String,
    /// Set for plain stackable items.
    pub count: Option<u32>,
    pub size: (u8, u8),
    /// Stackables are named by their code, other items uniquely.
    pub name: String,
}

impl FakeItem {
    fn line(&self) -> String {
        format!(
            "{} {} {} {} {}",
            self.code,
            self.count.map_or("-".into(), |n| n.to_string()),
            self.size.0,
            self.size.1,
            self.name
        )
    }

    fn parse(unit: u32, line: &str) -> FakeItem {
        let w: Vec<&str> = line.split(' ').collect();
        FakeItem {
            unit,
            code: w[0].into(),
            count: w[1].parse().ok(),
            size: (w[2].parse().unwrap(), w[3].parse().unwrap()),
            name: w[4].into(),
        }
    }
}

/// The fake game: one character, saved as text in its file.
pub(crate) struct FakeGame {
    pub name: String,
    pub path: PathBuf,
    pub items: Vec<FakeItem>,
    pub gold: u64,
    pub next_unit: u32,
    pub held: bool,
    /// Units the last `place` made, for `unplace`.
    placed: Vec<u32>,
}

impl FakeGame {
    pub fn new(dir: &Path, name: &str) -> FakeGame {
        FakeGame {
            name: name.into(),
            path: dir.join(format!("{name}.d2s")),
            items: Vec::new(),
            gold: 0,
            next_unit: 100,
            held: false,
            placed: Vec::new(),
        }
    }

    /// The character as its file says (a new process loading it).
    pub fn load(dir: &Path, name: &str) -> FakeGame {
        let mut g = FakeGame::new(dir, name);
        let text = std::fs::read_to_string(&g.path).unwrap();
        for line in text.lines() {
            if let Some(gold) = line.strip_prefix("gold ") {
                g.gold = gold.parse().unwrap();
            } else if let Some(item) = line.strip_prefix("item ") {
                let unit = g.unit();
                g.items.push(FakeItem::parse(unit, item));
            }
        }
        g
    }

    /// Writes the character's file (the game's own save).
    pub fn save(&mut self) {
        let bytes = self.record().unwrap();
        std::fs::write(&self.path, bytes).unwrap();
    }

    fn unit(&mut self) -> u32 {
        self.next_unit += 1;
        self.next_unit
    }

    pub fn add(&mut self, code: &str, count: Option<u32>, size: (u8, u8), name: &str) -> u32 {
        let unit = self.unit();
        self.items.push(FakeItem {
            unit,
            code: code.into(),
            count,
            size,
            name: name.into(),
        });
        unit
    }

    /// A rune, gem or other one-count stackable.
    pub fn stackable(&mut self, code: &str) -> u32 {
        self.add(code, Some(1), (1, 1), code)
    }

    pub fn units(&self) -> Vec<u32> {
        self.items.iter().map(|i| i.unit).collect()
    }

    /// Everything the character holds, by name (stackables by code, with
    /// their counts).
    pub fn totals(&self) -> BTreeMap<String, u64> {
        let mut t = BTreeMap::new();
        for i in &self.items {
            *t.entry(i.name.clone()).or_default() += u64::from(i.count.unwrap_or(1));
        }
        t
    }
}

impl LocalGame for FakeGame {
    fn character(&self) -> String {
        self.name.clone()
    }
    fn save_path(&self) -> PathBuf {
        self.path.clone()
    }
    fn hold_saves(&mut self, hold: bool) {
        self.held = hold;
    }
    fn kind(&mut self, unit: u32) -> Option<(String, Option<u32>)> {
        self.items
            .iter()
            .find(|i| i.unit == unit)
            .map(|i| (i.code.clone(), i.count))
    }
    fn lift(&mut self, what: Lift) -> Result<Lifted, GameError> {
        match what {
            Lift::Gold(g) => {
                if g > self.gold {
                    return Err(GameError::Refused("not enough gold".into()));
                }
                self.gold -= g;
                Ok(Lifted::Gold(g))
            }
            Lift::Unit(unit) => {
                let at = self
                    .items
                    .iter()
                    .position(|i| i.unit == unit)
                    .ok_or_else(|| GameError::Refused("no such item".into()))?;
                let i = self.items.remove(at);
                Ok(Lifted::Item {
                    unit,
                    record: i.line().into_bytes(),
                    facts: ItemFacts {
                        code: i.code.clone(),
                        item_type: "weap".into(),
                        name: i.name.clone(),
                        name_parts: vec![i.name.clone()],
                        ..ItemFacts::default()
                    },
                    size: i.size,
                    count: i.count,
                    look: Some(format!("look of {}", i.name).into_bytes()),
                })
            }
        }
    }
    fn restore(&mut self, lifted: &Lifted) -> Result<(), GameError> {
        match lifted {
            Lifted::Gold(g) => self.gold += g,
            Lifted::Item { unit, record, .. } => {
                let line = String::from_utf8(record.clone()).unwrap();
                self.items.push(FakeItem::parse(*unit, &line));
            }
        }
        Ok(())
    }
    fn room(&mut self, arrive: &Arrive, _: TakeTo) -> u64 {
        let free = INV_ITEMS.saturating_sub(self.items.len()) as u64;
        match arrive {
            Arrive::Item { .. } => free.min(1),
            Arrive::Stack { code, .. } if code == "key" => free * u64::from(KEY_STACK),
            Arrive::Stack { .. } => free,
            Arrive::Gold(_) => GOLD_MAX - self.gold,
        }
    }
    fn place(&mut self, arrive: &Arrive, to: TakeTo) -> Result<(), GameError> {
        if self.room(arrive, to) == 0 {
            return Err(GameError::NoRoom);
        }
        self.placed.clear();
        match arrive {
            Arrive::Gold(g) => self.gold += g,
            Arrive::Item { record } => {
                let unit = self.unit();
                let line = String::from_utf8(record.clone()).unwrap();
                self.items.push(FakeItem::parse(unit, &line));
                self.placed.push(unit);
            }
            Arrive::Stack { code, count } => {
                let mut left = *count;
                while left > 0 {
                    let n = if code == "key" {
                        left.min(KEY_STACK)
                    } else {
                        1
                    };
                    let unit = self.add(code, Some(n), (1, 1), code);
                    self.placed.push(unit);
                    left -= n;
                }
            }
        }
        Ok(())
    }
    fn unplace(&mut self, arrive: &Arrive, _: TakeTo) {
        if let Arrive::Gold(g) = arrive {
            self.gold -= g;
        }
        let placed = std::mem::take(&mut self.placed);
        self.items.retain(|i| !placed.contains(&i.unit));
    }
    fn record(&mut self) -> Result<Vec<u8>, GameError> {
        let mut s = format!("gold {}\n", self.gold);
        for i in &self.items {
            s.push_str(&format!("item {}\n", i.line()));
        }
        Ok(s.into_bytes())
    }
}

/// Everything in the stash, by the same names as [`FakeGame::totals`].
pub(crate) fn stash_totals(s: &mut LocalStash) -> BTreeMap<String, u64> {
    let mut t = BTreeMap::new();
    for page in 0..PAGES {
        for it in s.page(page).unwrap() {
            *t.entry(it.facts.unwrap().name).or_default() += 1;
        }
    }
    for (code, n) in s.stacks().unwrap() {
        *t.entry(code).or_default() += u64::from(n);
    }
    t
}

/// Character and stash together.
pub(crate) fn world_totals(g: &FakeGame, s: &mut LocalStash) -> BTreeMap<String, u64> {
    let mut t = g.totals();
    for (k, v) in stash_totals(s) {
        *t.entry(k).or_default() += v;
    }
    t.insert("gold".into(), g.gold + s.gold().unwrap());
    t
}

pub(crate) fn open(dir: &Path, hardcore: bool) -> LocalStash {
    LocalStash::open(dir, key(MOD, hardcore), catalog()).unwrap()
}

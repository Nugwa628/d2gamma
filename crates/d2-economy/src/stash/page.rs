//! The stash as a list (N28c): sorted, filtered and paged by the main
//! server, for the in-game panel now and the web later. Counts and gold
//! are small and come whole.

use rusqlite::types::Value;
use rusqlite::{params, params_from_iter, Connection, OptionalExtension, Row};

use crate::db::Economy;
use crate::error::Error;
use crate::key::{ItemId, Owner};
use crate::stash::{Item, ItemFacts};

/// Items per page.
pub const PAGE: u32 = 50;

/// What the items tab is sorted by (`StashOpen.sort`). Ties are broken by
/// the item ID, so pages never overlap.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum StashSort {
    Type,
    Quality,
    Name,
    Level,
    /// Date added (the default, newest first with `desc`).
    #[default]
    Date,
    /// A stat's value; items without the stat come last.
    Stat(u16),
}

/// A stat between `min` and `max`, both included.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct StatRange {
    pub stat: u16,
    pub min: i32,
    pub max: i32,
}

/// Filters for item lists (`ItemFilter`); every set field must match.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct ItemFilter {
    /// Part of a name (any of the item's names), any case.
    pub text: Option<String>,
    pub item_type: Option<String>,
    pub quality: Option<u8>,
    /// Level requirement range, both included.
    pub level: Option<(u8, u8)>,
    pub stats: Vec<StatRange>,
    pub ethereal: Option<bool>,
    /// Socket count range, both included.
    pub sockets: Option<(u8, u8)>,
    pub class: Option<u8>,
}

/// One request for a page of the items tab.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StashQuery {
    pub sort: StashSort,
    pub desc: bool,
    pub filter: ItemFilter,
    /// From 0.
    pub page: u16,
}

/// An item in a stash list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StashItem {
    pub item: Item,
    /// When it entered the economy, in milliseconds.
    pub added: i64,
}

/// A page of the items tab (`StashPage`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ItemsPage {
    /// Items matching the filter, on every page.
    pub total: u32,
    pub page: u16,
    pub items: Vec<StashItem>,
    pub gold: u64,
}

/// A `WHERE` clause being built, with its parameters.
#[derive(Default)]
pub(crate) struct Sql {
    pub clauses: Vec<String>,
    pub params: Vec<Value>,
}

impl Sql {
    pub fn push(&mut self, clause: impl Into<String>, values: impl IntoIterator<Item = Value>) {
        self.clauses.push(clause.into());
        self.params.extend(values);
    }

    pub fn where_clause(&self) -> String {
        if self.clauses.is_empty() {
            String::new()
        } else {
            format!(" WHERE {}", self.clauses.join(" AND "))
        }
    }
}

/// Where an item list's columns are: the table alias of the item columns
/// (`code`, `item_type`, ... as in `econ_items`), the stats table and the
/// key that joins them.
pub(crate) struct Cols<'a> {
    pub alias: &'a str,
    pub stats: &'a str,
    pub stats_key: &'a str,
    pub key: &'a str,
}

/// `%text%` for `LIKE ... ESCAPE '\'`.
fn like(text: &str) -> String {
    let mut s = String::from("%");
    for c in text.chars() {
        if matches!(c, '%' | '_' | '\\') {
            s.push('\\');
        }
        s.push(c);
    }
    s.push('%');
    s
}

/// Adds every filter but the text (which lists search differently) to
/// `sql`.
pub(crate) fn push_filter(sql: &mut Sql, f: &ItemFilter, c: &Cols<'_>) {
    let a = c.alias;
    if let Some(t) = &f.item_type {
        sql.push(format!("{a}.item_type = ?"), [Value::Text(t.clone())]);
    }
    if let Some(q) = f.quality {
        sql.push(format!("{a}.quality = ?"), [Value::Integer(q.into())]);
    }
    if let Some((lo, hi)) = f.level {
        sql.push(
            format!("{a}.level_req BETWEEN ? AND ?"),
            [Value::Integer(lo.into()), Value::Integer(hi.into())],
        );
    }
    if let Some(e) = f.ethereal {
        sql.push(format!("{a}.ethereal = ?"), [Value::Integer(e.into())]);
    }
    if let Some((lo, hi)) = f.sockets {
        sql.push(
            format!("{a}.sockets BETWEEN ? AND ?"),
            [Value::Integer(lo.into()), Value::Integer(hi.into())],
        );
    }
    if let Some(class) = f.class {
        sql.push(format!("{a}.class = ?"), [Value::Integer(class.into())]);
    }
    for r in &f.stats {
        sql.push(
            format!(
                "{key} IN (SELECT {sk} FROM {st} WHERE stat = ? AND value BETWEEN ? AND ?)",
                key = c.key,
                sk = c.stats_key,
                st = c.stats
            ),
            [
                Value::Integer(r.stat.into()),
                Value::Integer(r.min.into()),
                Value::Integer(r.max.into()),
            ],
        );
    }
}

/// The item columns as `load_facts` reads them, under alias `a`.
pub(crate) fn fact_columns(a: &str) -> String {
    format!(
        "{a}.item_id, {a}.record, {a}.code, {a}.item_type, {a}.quality, {a}.name, {a}.names, \
         {a}.level_req, {a}.ilvl, {a}.sockets, {a}.ethereal, {a}.class, {a}.runeword, \
         {a}.identified, {a}.types"
    )
}

/// Reads an item from `fact_columns` starting at column `at`; its stats
/// are filled by [`fill_stats`].
pub(crate) fn item_of(r: &Row, at: usize) -> rusqlite::Result<Item> {
    let names: String = r.get(at + 6)?;
    let types: String = r.get(at + 14)?;
    Ok(Item {
        id: ItemId(r.get(at)?),
        record: r.get(at + 1)?,
        facts: ItemFacts {
            code: r.get(at + 2)?,
            item_type: r.get(at + 3)?,
            quality: r.get(at + 4)?,
            name: r.get(at + 5)?,
            name_parts: names.split_whitespace().map(str::to_owned).collect(),
            level_req: r.get(at + 7)?,
            ilvl: r.get(at + 8)?,
            sockets: r.get(at + 9)?,
            ethereal: r.get(at + 10)?,
            class: r.get(at + 11)?,
            stats: Vec::new(),
            runeword: r.get(at + 12)?,
            identified: r.get(at + 13)?,
            types: types.split_whitespace().map(str::to_owned).collect(),
        },
    })
}

pub(crate) fn fill_stats(conn: &Connection, item: &mut Item) -> Result<(), Error> {
    let mut st =
        conn.prepare_cached("SELECT stat, value FROM item_stats WHERE item_id = ?1 ORDER BY stat")?;
    let rows = st.query_map([item.id.0], |r| Ok((r.get(0)?, r.get(1)?)))?;
    item.facts.stats = rows.collect::<Result<_, _>>()?;
    Ok(())
}

/// An item in the economy, wherever it is.
pub(crate) fn load_item(conn: &Connection, id: ItemId) -> Result<Option<Item>, Error> {
    let item = conn
        .query_row(
            &format!(
                "SELECT {} FROM econ_items i WHERE i.item_id = ?1",
                fact_columns("i")
            ),
            [id.0],
            |r| item_of(r, 0),
        )
        .optional()?;
    match item {
        Some(mut item) => {
            fill_stats(conn, &mut item)?;
            Ok(Some(item))
        }
        None => Ok(None),
    }
}

impl Economy {
    /// A page of an account's items tab.
    pub fn stash_items(&self, owner: &Owner, q: &StashQuery) -> Result<ItemsPage, Error> {
        self.check_key(&owner.economy)?;
        let split = owner.economy.split();
        let conn = self.conn();
        let mut sql = Sql::default();
        sql.push(
            "i.account = ? AND i.economy = ? AND i.place = 'stash'",
            [Value::Integer(owner.account.0), Value::Text(split.clone())],
        );
        if let Some(t) = q.filter.text.as_deref().filter(|t| !t.trim().is_empty()) {
            let pat = like(t.trim());
            sql.push(
                "(i.name LIKE ? ESCAPE '\\' OR i.names LIKE ? ESCAPE '\\')",
                [Value::Text(pat.clone()), Value::Text(pat)],
            );
        }
        push_filter(
            &mut sql,
            &q.filter,
            &Cols {
                alias: "i",
                stats: "item_stats",
                stats_key: "item_id",
                key: "i.item_id",
            },
        );
        let wh = sql.where_clause();
        let total: i64 = conn.query_row(
            &format!("SELECT COUNT(*) FROM econ_items i{wh}"),
            params_from_iter(sql.params.iter()),
            |r| r.get(0),
        )?;
        let dir = if q.desc { "DESC" } else { "ASC" };
        let mut order_params = Vec::new();
        let order = match q.sort {
            StashSort::Type => format!("i.item_type {dir}, i.name COLLATE NOCASE {dir}"),
            StashSort::Quality => format!("i.quality {dir}"),
            StashSort::Name => format!("i.name COLLATE NOCASE {dir}"),
            StashSort::Level => format!("i.level_req {dir}"),
            StashSort::Date => format!("i.added {dir}"),
            StashSort::Stat(stat) => {
                order_params.push(Value::Integer(stat.into()));
                format!(
                    "(SELECT value FROM item_stats s WHERE s.item_id = i.item_id AND s.stat = ?) \
                     {dir} NULLS LAST"
                )
            }
        };
        let mut all = order_params;
        all.splice(0..0, sql.params.iter().cloned());
        all.push(Value::Integer(PAGE.into()));
        all.push(Value::Integer(i64::from(q.page) * i64::from(PAGE)));
        let mut st = conn.prepare(&format!(
            "SELECT {}, i.added FROM econ_items i{wh} ORDER BY {order}, i.item_id {dir} \
             LIMIT ? OFFSET ?",
            fact_columns("i")
        ))?;
        let rows = st.query_map(params_from_iter(all.iter()), |r| {
            Ok(StashItem {
                item: item_of(r, 0)?,
                added: r.get(15)?,
            })
        })?;
        let mut items: Vec<StashItem> = rows.collect::<Result<_, _>>()?;
        for it in &mut items {
            fill_stats(conn, &mut it.item)?;
        }
        Ok(ItemsPage {
            total: u32::try_from(total).unwrap_or(u32::MAX),
            page: q.page,
            items,
            gold: self.stash_gold(owner)?,
        })
    }

    /// The stackables tab: every code the account holds, with its count,
    /// by code.
    pub fn stash_stacks(&self, owner: &Owner) -> Result<Vec<(String, u32)>, Error> {
        self.check_key(&owner.economy)?;
        let mut st = self.conn().prepare(
            "SELECT code, count FROM stash_stacks WHERE account = ?1 AND economy = ?2 ORDER BY code",
        )?;
        let rows = st.query_map(params![owner.account.0, owner.economy.split()], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// The record of item `id` in the account's stash of this economy
    /// (`None` when it isn't there), for the main server to read its facts.
    pub fn stash_record(&self, owner: &Owner, id: ItemId) -> Result<Option<Vec<u8>>, Error> {
        self.check_key(&owner.economy)?;
        Ok(self
            .conn()
            .query_row(
                "SELECT record FROM econ_items WHERE item_id = ?1 AND account = ?2                  AND economy = ?3 AND place = 'stash'",
                params![id.0, owner.account.0, owner.economy.split()],
                |r| r.get(0),
            )
            .optional()?)
    }

    /// The account's gold balance in this economy.
    pub fn stash_gold(&self, owner: &Owner) -> Result<u64, Error> {
        self.check_key(&owner.economy)?;
        let gold: Option<i64> = self
            .conn()
            .query_row(
                "SELECT gold FROM stash_gold WHERE account = ?1 AND economy = ?2",
                params![owner.account.0, owner.economy.split()],
                |r| r.get(0),
            )
            .optional()?;
        Ok(gold.unwrap_or(0) as u64)
    }
}

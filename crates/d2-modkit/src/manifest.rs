//! The mod manifest: `mod.toml`.
//!
//! Every mod, as a folder or as a pack, starts with this file. It says who
//! the mod is, which modding API it was made for, and how it relates to
//! other mods:
//!
//! ```toml
//! id = "ember-age"              # lowercase slug: a-z, 0-9 and single hyphens
//! name = "Ember Age"            # shown to players (also the window title)
//! version = "1.2.0"             # MAJOR.MINOR.PATCH
//! author = "Some Modder"
//! description = "Harder fire, new runes."
//! api = 1                       # the modding API version the mod targets
//! kind = "full"                 # "full" or "addon"
//!
//! allow_derived = true          # others may build on this mod (full mods)
//! approved_addons = ["ember-ui"] # add-ons this mod's maker approves (full mods)
//!
//! [base]                        # a full mod built on another mod
//! id = "ember-age"
//! version = ">=1.0.0, <2.0.0"
//! ```
//!
//! Two more keys carry over from the layer stack: `native_format_version`
//! (optional here) and `requires`. A `mod.toml` with none of the new keys and
//! no `api` is a layer-only mod from before the kit; [`parse_mod_toml`] keeps
//! reading it exactly as before.
//!
//! An add-on has no `[base]`; it names what it attaches to with
//! `for = "<mod id>"` or `for = "d2gamma"`, and may set `screen_only = true`
//! (changes only what is on screen, never rules; the only kind that can run
//! on plain D2Gamma).
//!
//! Parsing is strict: an unknown key, a wrong type or a key that does not
//! belong to the mod's kind is an error that names the field. All problems
//! are reported together so a modder fixes them in one pass.

use std::fmt;

use crate::version::{Version, VersionReq};

/// The file name of the manifest, in a mod folder and inside a pack.
pub const MANIFEST_FILE: &str = "mod.toml";
/// The `for` value of an add-on that attaches to plain D2Gamma.
pub const TARGET_D2GAMMA: &str = "d2gamma";

/// Ids no mod may use: they name the base game.
const RESERVED_IDS: &[&str] = &[TARGET_D2GAMMA, "d2rust", "base", "default", "none"];
const MAX_ID: usize = 40;
const MIN_ID: usize = 3;
const MAX_NAME: usize = 64;
const MAX_AUTHOR: usize = 64;
const MAX_DESCRIPTION: usize = 2000;

const KEYS: &[&str] = &[
    "id",
    "name",
    "version",
    "author",
    "description",
    "api",
    "kind",
    "base",
    "allow_derived",
    "approved_addons",
    "for",
    "screen_only",
    "native_format_version",
    "requires",
    "market",
    "ladder",
    "accepts_d2gamma_characters",
];

/// The keys a layer-only `mod.toml` (one from before the mod kit, with no
/// `api`) may hold.
const LEGACY_KEYS: &[&str] = &["name", "version", "native_format_version", "requires"];

/// Whether a mod replaces the game's content or attaches to another mod.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A mod that runs by itself over the base data.
    Full,
    /// A small mod that attaches to a full mod (or to plain D2Gamma, when
    /// it is screen-only).
    Addon,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Full => "full",
            Kind::Addon => "addon",
        }
    }
}

/// The mod a full mod builds on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BaseRef {
    /// The other mod's id.
    pub id: String,
    /// The versions of it this mod works with.
    pub version: VersionReq,
}

/// A parsed, validated `mod.toml`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Manifest {
    pub id: String,
    pub name: String,
    pub version: Version,
    pub author: String,
    pub description: String,
    /// The modding API version the mod targets (see [`crate::api`]).
    pub api: u32,
    pub kind: Kind,
    /// Full mods: online services the maker permits (both true by default).
    pub market: bool,
    pub ladder: bool,
    /// Full mods: allow an explicit copy of a plain D2Gamma character.
    pub accepts_d2gamma_characters: bool,
    /// Full mods: the mod this one builds on.
    pub base: Option<BaseRef>,
    /// Full mods: whether others may build on this mod.
    pub allow_derived: bool,
    /// Full mods: ids of the add-ons the maker approves.
    pub approved_addons: Vec<String>,
    /// Add-ons: a mod id or `"d2gamma"`: what the add-on attaches to.
    pub attaches_to: Option<String>,
    /// Add-ons: changes only what is on screen, never rules.
    pub screen_only: bool,
    /// The native file layout version the mod's files follow. Optional in a
    /// kit mod (left out, it means this build's); when written it must match.
    pub native_format_version: Option<u32>,
    /// Ids of mods that must be loaded before this one.
    pub requires: Vec<String>,
}

/// A layer-only `mod.toml`, from before the mod kit: it has no `api` and
/// keeps exactly the keys the layer stack always read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacyInfo {
    pub name: String,
    /// Free text; layer-only mods never had to use semantic versions.
    pub version: String,
    pub native_format_version: u32,
    pub requires: Vec<String>,
}

/// What a `mod.toml` turned out to be.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModToml {
    /// A mod-kit manifest (it names an `api`).
    Kit(Box<Manifest>),
    /// A layer-only `mod.toml` that keeps loading as it always did.
    Legacy(LegacyInfo),
}

/// Reads any `mod.toml`: a file with only the old layer keys and no `api`
/// is a layer-only mod, anything else is a mod-kit manifest and is held to
/// every rule of one.
pub fn parse_mod_toml(text: &str) -> Result<ModToml, ManifestError> {
    let table: toml::Table = text.parse().map_err(|e: toml::de::Error| ManifestError {
        problems: vec![Problem {
            field: String::new(),
            message: format!("not valid TOML: {}", e.message()),
        }],
    })?;
    // Any key the old layer files never had makes it a kit manifest.
    let kit = table
        .keys()
        .any(|k| KEYS.contains(&k.as_str()) && !LEGACY_KEYS.contains(&k.as_str()));
    if kit {
        return Manifest::parse(text).map(|m| ModToml::Kit(Box::new(m)));
    }
    let mut r = Reader {
        table: &table,
        problems: Vec::new(),
    };
    for k in table.keys() {
        if !LEGACY_KEYS.contains(&k.as_str()) {
            r.problem(k, "unknown key");
        }
    }
    let name = r.string("name", true);
    let version = r.string("version", true);
    let nfv = r.format_version(true);
    let requires = r.string_list("requires");
    match (name, version, nfv) {
        (Some(name), Some(version), Some(native_format_version)) if r.problems.is_empty() => {
            Ok(ModToml::Legacy(LegacyInfo {
                name,
                version,
                native_format_version,
                requires,
            }))
        }
        _ => Err(ManifestError {
            problems: r.problems,
        }),
    }
}

/// One problem in a manifest: the field and what is wrong with it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    /// The key (`id`, `base.version`, ...), or empty for the whole file.
    pub field: String,
    pub message: String,
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.field.is_empty() {
            write!(f, "{MANIFEST_FILE}: {}", self.message)
        } else {
            write!(f, "{MANIFEST_FILE}: `{}`: {}", self.field, self.message)
        }
    }
}

/// Every problem found in a manifest (never empty).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestError {
    pub problems: Vec<Problem>,
}

impl ManifestError {
    /// The problems as plain text (`` `field` message ``), without the file
    /// name, for callers that name the file themselves.
    pub fn detail(&self) -> String {
        self.problems
            .iter()
            .map(|p| {
                if p.field.is_empty() {
                    p.message.clone()
                } else {
                    format!("`{}` {}", p.field, p.message)
                }
            })
            .collect::<Vec<_>>()
            .join("; ")
    }

    /// Whether some problem is about `field`.
    pub fn mentions(&self, field: &str) -> bool {
        self.problems.iter().any(|p| p.field == field)
    }
}

impl fmt::Display for ManifestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, p) in self.problems.iter().enumerate() {
            if i > 0 {
                writeln!(f)?;
            }
            write!(f, "{p}")?;
        }
        Ok(())
    }
}

impl std::error::Error for ManifestError {}

/// Whether `id` is a valid mod id; the error says why not.
pub fn check_id(id: &str) -> Result<(), String> {
    if id.len() < MIN_ID || id.len() > MAX_ID {
        return Err(format!(
            "`{id}` must be {MIN_ID} to {MAX_ID} characters long"
        ));
    }
    if !id
        .bytes()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    {
        return Err(format!(
            "`{id}` may only use lowercase letters a-z, digits and hyphens"
        ));
    }
    if !id.as_bytes()[0].is_ascii_lowercase() {
        return Err(format!("`{id}` must start with a letter"));
    }
    if id.ends_with('-') || id.contains("--") {
        return Err(format!(
            "`{id}` must not end with a hyphen or have two hyphens in a row"
        ));
    }
    if RESERVED_IDS.contains(&id) {
        return Err(format!("`{id}` is reserved"));
    }
    Ok(())
}

struct Reader<'a> {
    table: &'a toml::Table,
    problems: Vec<Problem>,
}

impl Reader<'_> {
    fn problem(&mut self, field: &str, message: impl Into<String>) {
        self.problems.push(Problem {
            field: field.to_owned(),
            message: message.into(),
        });
    }

    fn string(&mut self, key: &str, required: bool) -> Option<String> {
        match self.table.get(key) {
            None => {
                if required {
                    self.problem(key, "is missing");
                }
                None
            }
            Some(toml::Value::String(s)) => Some(s.clone()),
            Some(other) => {
                self.problem(key, format!("must be text, not {}", other.type_str()));
                None
            }
        }
    }

    /// `native_format_version`: a whole number; `required` says whether
    /// leaving it out is a problem.
    fn format_version(&mut self, required: bool) -> Option<u32> {
        let key = "native_format_version";
        match self.table.get(key) {
            None => {
                if required {
                    self.problem(key, "is missing");
                }
                None
            }
            Some(toml::Value::Integer(n)) => match u32::try_from(*n) {
                Ok(n) => Some(n),
                Err(_) => {
                    self.problem(key, "is not a valid version number");
                    None
                }
            },
            Some(other) => {
                self.problem(
                    key,
                    format!("must be a whole number, not {}", other.type_str()),
                );
                None
            }
        }
    }

    /// A list of text values; a missing key is an empty list.
    fn string_list(&mut self, key: &str) -> Vec<String> {
        match self.table.get(key) {
            None => Vec::new(),
            Some(toml::Value::Array(items)) => {
                let mut out = Vec::new();
                for (i, item) in items.iter().enumerate() {
                    match item {
                        toml::Value::String(s) => out.push(s.clone()),
                        other => self.problem(
                            key,
                            format!("entry {} must be text, not {}", i + 1, other.type_str()),
                        ),
                    }
                }
                out
            }
            Some(other) => {
                self.problem(
                    key,
                    format!("must be a list of text, not {}", other.type_str()),
                );
                Vec::new()
            }
        }
    }

    fn boolean(&mut self, key: &str) -> Option<bool> {
        match self.table.get(key) {
            None => None,
            Some(toml::Value::Boolean(b)) => Some(*b),
            Some(other) => {
                self.problem(
                    key,
                    format!("must be true or false, not {}", other.type_str()),
                );
                None
            }
        }
    }
}

fn plain_text(s: &str, multiline: bool) -> bool {
    s.chars()
        .all(|c| !c.is_control() || (multiline && (c == '\n' || c == '\r' || c == '\t')))
}

impl Manifest {
    /// Parses and validates the text of a `mod.toml`.
    pub fn parse(text: &str) -> Result<Manifest, ManifestError> {
        let table: toml::Table = match text.parse() {
            Ok(t) => t,
            Err(e) => {
                return Err(ManifestError {
                    problems: vec![Problem {
                        field: String::new(),
                        message: format!("not valid TOML: {}", e.message()),
                    }],
                })
            }
        };
        let mut r = Reader {
            table: &table,
            problems: Vec::new(),
        };

        for k in table.keys() {
            if !KEYS.contains(&k.as_str()) {
                r.problem(k, format!("unknown key (known keys: {})", KEYS.join(", ")));
            }
        }

        let id = r.string("id", true);
        if let Some(id) = &id {
            if let Err(e) = check_id(id) {
                r.problem("id", e);
            }
        }
        let name = r.string("name", true);
        if let Some(n) = &name {
            if n.trim().is_empty() {
                r.problem("name", "must not be empty");
            } else if n.chars().count() > MAX_NAME || !plain_text(n, false) {
                r.problem(
                    "name",
                    format!("must be plain text of at most {MAX_NAME} characters"),
                );
            }
        }
        let version = match r.string("version", true) {
            Some(s) => match Version::parse(&s) {
                Ok(v) => Some(v),
                Err(e) => {
                    r.problem("version", e);
                    None
                }
            },
            None => None,
        };
        let author = r.string("author", true);
        if let Some(a) = &author {
            if a.trim().is_empty() {
                r.problem("author", "must not be empty");
            } else if a.chars().count() > MAX_AUTHOR || !plain_text(a, false) {
                r.problem(
                    "author",
                    format!("must be plain text of at most {MAX_AUTHOR} characters"),
                );
            }
        }
        let description = r.string("description", true);
        if let Some(d) = &description {
            if d.chars().count() > MAX_DESCRIPTION || !plain_text(d, true) {
                r.problem(
                    "description",
                    format!("must be text of at most {MAX_DESCRIPTION} characters"),
                );
            }
        }
        let api = match table.get("api") {
            None => {
                r.problem(
                    "api",
                    "is missing (the modding API version the mod targets)",
                );
                None
            }
            Some(toml::Value::Integer(n)) => match u32::try_from(*n) {
                Ok(n) if n >= 1 => Some(n),
                _ => {
                    r.problem(
                        "api",
                        format!("{n} is not a valid API version (1 or higher)"),
                    );
                    None
                }
            },
            Some(other) => {
                r.problem(
                    "api",
                    format!("must be a whole number, not {}", other.type_str()),
                );
                None
            }
        };
        let kind = match r.string("kind", true).as_deref() {
            Some("full") => Some(Kind::Full),
            Some("addon") => Some(Kind::Addon),
            Some(other) => {
                r.problem(
                    "kind",
                    format!("`{other}` is not a kind: use \"full\" or \"addon\""),
                );
                None
            }
            None => None,
        };

        let allow_derived = r.boolean("allow_derived");
        let screen_only = r.boolean("screen_only");
        let market = r.boolean("market");
        let ladder = r.boolean("ladder");
        let accepts_d2gamma_characters = r.boolean("accepts_d2gamma_characters");
        if kind == Some(Kind::Addon) && accepts_d2gamma_characters.is_some() {
            r.problem(
                "accepts_d2gamma_characters",
                "only the full mod may permit character copies",
            );
        }
        if kind == Some(Kind::Addon) && (market.is_some() || ladder.is_some()) {
            r.problem(
                "market/ladder",
                "only the full mod may set online service policy",
            );
        }
        let native_format_version = r.format_version(false);
        let requires = r.string_list("requires");
        for req in &requires {
            if let Err(e) = check_id(req) {
                r.problem("requires", e);
            }
        }

        let base = match table.get("base") {
            None => None,
            Some(toml::Value::Table(t)) => {
                let mut sub = Reader {
                    table: t,
                    problems: Vec::new(),
                };
                for k in t.keys() {
                    if k != "id" && k != "version" {
                        sub.problem(&format!("base.{k}"), "unknown key (use `id` and `version`)");
                    }
                }
                let bid = sub.string("id", true);
                let bver = sub.string("version", true);
                let mut base_problems = Vec::new();
                for p in sub.problems {
                    base_problems.push(Problem {
                        field: if p.field.starts_with("base.") {
                            p.field
                        } else {
                            format!("base.{}", p.field)
                        },
                        message: p.message,
                    });
                }
                r.problems.extend(base_problems);
                if let Some(b) = &bid {
                    if let Err(e) = check_id(b) {
                        r.problem("base.id", e);
                    }
                }
                let req = match bver {
                    Some(s) => match VersionReq::parse(&s) {
                        Ok(q) => Some(q),
                        Err(e) => {
                            r.problem("base.version", e);
                            None
                        }
                    },
                    None => None,
                };
                match (bid, req) {
                    (Some(id), Some(version)) => Some(BaseRef { id, version }),
                    _ => None,
                }
            }
            Some(other) => {
                r.problem(
                    "base",
                    format!(
                        "must be a table with `id` and `version`, not {}",
                        other.type_str()
                    ),
                );
                None
            }
        };

        let mut approved_addons = Vec::new();
        match table.get("approved_addons") {
            None => {}
            Some(toml::Value::Array(items)) => {
                for (i, item) in items.iter().enumerate() {
                    match item {
                        toml::Value::String(s) => {
                            if let Err(e) = check_id(s) {
                                r.problem("approved_addons", format!("entry {}: {e}", i + 1));
                            } else if approved_addons.contains(s) {
                                r.problem("approved_addons", format!("`{s}` is listed twice"));
                            } else {
                                approved_addons.push(s.clone());
                            }
                        }
                        other => r.problem(
                            "approved_addons",
                            format!(
                                "entry {} must be an add-on id (text), not {}",
                                i + 1,
                                other.type_str()
                            ),
                        ),
                    }
                }
            }
            Some(other) => r.problem(
                "approved_addons",
                format!("must be a list of add-on ids, not {}", other.type_str()),
            ),
        }

        let attaches_to = r.string("for", false);
        if let Some(t) = &attaches_to {
            if t != TARGET_D2GAMMA {
                if let Err(e) = check_id(t) {
                    r.problem("for", e);
                }
            }
        }

        // Which keys belong to which kind.
        match kind {
            Some(Kind::Full) => {
                for key in ["for", "screen_only"] {
                    if table.contains_key(key) {
                        r.problem(key, "only add-ons use this (this mod is `kind = \"full\"`)");
                    }
                }
            }
            Some(Kind::Addon) => {
                for key in ["base", "allow_derived", "approved_addons"] {
                    if table.contains_key(key) {
                        r.problem(
                            key,
                            "only full mods use this (this mod is `kind = \"addon\"`)",
                        );
                    }
                }
                if attaches_to.is_none() && !table.contains_key("for") {
                    r.problem(
                        "for",
                        "is missing: an add-on says what it attaches to (a mod id or \"d2gamma\")",
                    );
                }
                if attaches_to.as_deref() == Some(TARGET_D2GAMMA) && screen_only != Some(true) {
                    r.problem(
                        "screen_only",
                        "an add-on for plain D2Gamma must set `screen_only = true` (only screen changes may run there)",
                    );
                }
            }
            None => {}
        }

        // Self references.
        if let Some(id) = &id {
            if base.as_ref().is_some_and(|b| &b.id == id) {
                r.problem("base.id", "a mod cannot build on itself");
            }
            if attaches_to.as_ref() == Some(id) {
                r.problem("for", "an add-on cannot attach to itself");
            }
            if approved_addons.contains(id) {
                r.problem("approved_addons", "a mod cannot approve itself");
            }
        }

        if !r.problems.is_empty() {
            return Err(ManifestError {
                problems: r.problems,
            });
        }
        // Every required value is present when there are no problems.
        match (id, name, version, author, description, api, kind) {
            (
                Some(id),
                Some(name),
                Some(version),
                Some(author),
                Some(description),
                Some(api),
                Some(kind),
            ) => Ok(Manifest {
                id,
                name,
                version,
                author,
                description,
                api,
                kind,
                market: market.unwrap_or(true),
                ladder: ladder.unwrap_or(true),
                accepts_d2gamma_characters: accepts_d2gamma_characters.unwrap_or(false),
                base,
                allow_derived: allow_derived.unwrap_or(false),
                approved_addons,
                attaches_to,
                screen_only: screen_only.unwrap_or(false),
                native_format_version,
                requires,
            }),
            _ => Err(ManifestError {
                problems: vec![Problem {
                    field: String::new(),
                    message: "incomplete manifest".into(),
                }],
            }),
        }
    }

    /// The manifest as `mod.toml` text; [`Manifest::parse`] reads it back
    /// to an equal value.
    pub fn to_toml(&self) -> String {
        let mut out = String::new();
        let q = |s: &str| toml::Value::String(s.to_owned()).to_string();
        out.push_str(&format!("id = {}\n", q(&self.id)));
        out.push_str(&format!("name = {}\n", q(&self.name)));
        out.push_str(&format!("version = {}\n", q(&self.version.to_string())));
        out.push_str(&format!("author = {}\n", q(&self.author)));
        out.push_str(&format!("description = {}\n", q(&self.description)));
        out.push_str(&format!("api = {}\n", self.api));
        out.push_str(&format!("kind = {}\n", q(self.kind.as_str())));
        if let Some(n) = self.native_format_version {
            out.push_str(&format!("native_format_version = {n}\n"));
        }
        if !self.requires.is_empty() {
            let items: Vec<String> = self.requires.iter().map(|a| q(a)).collect();
            out.push_str(&format!("requires = [{}]\n", items.join(", ")));
        }
        match self.kind {
            Kind::Full => {
                if self.accepts_d2gamma_characters {
                    out.push_str("accepts_d2gamma_characters = true\n");
                }
                if !self.market {
                    out.push_str("market = false\n");
                }
                if !self.ladder {
                    out.push_str("ladder = false\n");
                }
                out.push_str(&format!("allow_derived = {}\n", self.allow_derived));
                if !self.approved_addons.is_empty() {
                    let items: Vec<String> = self.approved_addons.iter().map(|a| q(a)).collect();
                    out.push_str(&format!("approved_addons = [{}]\n", items.join(", ")));
                }
            }
            Kind::Addon => {
                if let Some(t) = &self.attaches_to {
                    out.push_str(&format!("for = {}\n", q(t)));
                }
                out.push_str(&format!("screen_only = {}\n", self.screen_only));
            }
        }
        if let Some(b) = &self.base {
            out.push_str(&format!(
                "\n[base]\nid = {}\nversion = {}\n",
                q(&b.id),
                q(b.version.as_str())
            ));
        }
        out
    }

    /// Whether this mod may build on `other`: `other` must allow derived
    /// mods, be the mod this one names, and be in the version range.
    pub fn check_builds_on(&self, other: &Manifest) -> Result<(), String> {
        let Some(base) = &self.base else {
            return Err(format!("`{}` does not name a mod to build on", self.id));
        };
        if base.id != other.id {
            return Err(format!(
                "`{}` builds on `{}`, not on `{}`",
                self.id, base.id, other.id
            ));
        }
        if other.kind != Kind::Full {
            return Err(format!(
                "`{}` is an add-on; only full mods can be built on",
                other.id
            ));
        }
        if !other.allow_derived {
            return Err(format!(
                "`{}` by {} does not allow other mods to build on it (`allow_derived`)",
                other.id, other.author
            ));
        }
        if !base.version.matches(&other.version) {
            return Err(format!(
                "`{}` needs `{}` {} but version {} is installed",
                self.id, other.id, base.version, other.version
            ));
        }
        Ok(())
    }

    /// Whether this add-on may run with `target` (`None` for plain
    /// D2Gamma): it must attach to it, and the target's maker must have
    /// approved it, or (plain D2Gamma) it must be screen-only.
    pub fn check_addon_for(&self, target: Option<&Manifest>) -> Result<(), String> {
        if self.kind != Kind::Addon {
            return Err(format!("`{}` is a full mod, not an add-on", self.id));
        }
        let attach = self.attaches_to.as_deref().unwrap_or("");
        match target {
            None => {
                if attach != TARGET_D2GAMMA {
                    return Err(format!(
                        "`{}` is an add-on for `{attach}` and cannot run on plain D2Gamma",
                        self.id
                    ));
                }
                if !self.screen_only {
                    return Err(format!(
                        "`{}` changes more than the screen, so it cannot run on plain D2Gamma",
                        self.id
                    ));
                }
                Ok(())
            }
            Some(t) => {
                if attach != t.id {
                    return Err(format!(
                        "`{}` is an add-on for `{attach}`, not for `{}`",
                        self.id, t.id
                    ));
                }
                if !t.approved_addons.contains(&self.id) {
                    return Err(format!(
                        "`{}` by {} has not approved the add-on `{}`",
                        t.id, t.author, self.id
                    ));
                }
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FULL: &str = r#"
id = "ember-age"
name = "Ember Age"
version = "1.2.0"
author = "Some Modder"
description = "Harder fire, new runes."
api = 1
kind = "full"
allow_derived = true
approved_addons = ["ember-ui"]
"#;

    const ADDON: &str = r#"
id = "ember-ui"
name = "Ember UI"
version = "0.1.0"
author = "Other Modder"
description = "A bigger belt."
api = 1
kind = "addon"
for = "ember-age"
"#;

    fn err(text: &str) -> ManifestError {
        Manifest::parse(text).expect_err("should be invalid")
    }

    fn replace(base: &str, from: &str, to: &str) -> String {
        assert!(base.contains(from), "{from} not in base");
        base.replace(from, to)
    }

    #[test]
    fn character_copies_are_opt_in_and_only_the_full_mod_can_allow_them() {
        let original = Manifest::parse(FULL).unwrap();
        assert!(!original.accepts_d2gamma_characters);
        assert!(!original.to_toml().contains("accepts_d2gamma_characters"));
        for value in ["true", "false"] {
            let parsed =
                Manifest::parse(&format!("{FULL}\naccepts_d2gamma_characters={value}")).unwrap();
            assert_eq!(parsed.accepts_d2gamma_characters, value == "true");
            assert_eq!(Manifest::parse(&parsed.to_toml()).unwrap(), parsed);
            assert!(
                Manifest::parse(&format!("{ADDON}\naccepts_d2gamma_characters={value}")).is_err()
            );
        }
        assert!(Manifest::parse(&format!("{FULL}\naccepts_d2gamma_characters='yes'")).is_err());
    }

    #[test]
    fn online_services_default_enabled_and_full_mod_can_disable_them() {
        assert!(Manifest::parse(&FULL.replace("id = \"ember-age\"", "id = \"d2rust\"")).is_err());
        let original = Manifest::parse(FULL).unwrap();
        assert!(original.market && original.ladder);
        let text = FULL.replace(
            "kind = \"full\"",
            "kind = \"full\"\nmarket = false\nladder = false",
        );
        let disabled = Manifest::parse(&text).unwrap();
        assert!(!disabled.market && !disabled.ladder);
        assert_eq!(Manifest::parse(&disabled.to_toml()).unwrap(), disabled);
        assert!(Manifest::parse(
            &FULL.replace("kind = \"full\"", "kind = \"full\"\nmarket = 'off'")
        )
        .is_err());
    }

    #[test]
    fn layer_only_files_keep_loading() {
        let old = "name = \"a\"\nversion = \"1\"\nnative_format_version = 1\nrequires = [\"x\"]\n";
        let ModToml::Legacy(l) = parse_mod_toml(old).unwrap() else {
            panic!("an old file is layer-only")
        };
        assert_eq!((l.name.as_str(), l.version.as_str()), ("a", "1"));
        assert_eq!(l.native_format_version, 1);
        assert_eq!(l.requires, ["x"]);
        // `requires` is optional, the format version is not, unknown keys are errors.
        let ok = "name = \"a\"\nversion = \"1\"\nnative_format_version = 1\n";
        assert!(matches!(parse_mod_toml(ok), Ok(ModToml::Legacy(_))));
        let e = parse_mod_toml("name = \"a\"\nversion = \"1\"\n").unwrap_err();
        assert!(e.mentions("native_format_version"), "{e}");
        let e =
            parse_mod_toml("name = \"a\"\nversion = 1\nnative_format_version = 1\n").unwrap_err();
        assert!(e.mentions("version"), "{e}");
        // An unknown key in an old-looking file is held to the kit's rules.
        let e = parse_mod_toml(&format!("{ok}extra = 1\n")).unwrap_err();
        assert!(e.mentions("extra"), "{e}");
        // With an api it is a kit manifest, and kit rules apply.
        assert!(matches!(parse_mod_toml(FULL), Ok(ModToml::Kit(_))));
        assert!(parse_mod_toml(&format!("{ok}api = 1\n")).is_err());
        assert!(!e.detail().is_empty());
    }

    #[test]
    fn kit_files_may_carry_the_layer_keys() {
        let text = format!("{FULL}native_format_version = 1\nrequires = [\"other-mod\"]\n");
        let m = Manifest::parse(&text).unwrap();
        assert_eq!(m.native_format_version, Some(1));
        assert_eq!(m.requires, ["other-mod"]);
        assert_eq!(Manifest::parse(&m.to_toml()).unwrap(), m);
        assert!(Manifest::parse(FULL)
            .unwrap()
            .native_format_version
            .is_none());
        assert!(err(&format!("{FULL}requires = [\"Bad Id\"]\n")).mentions("requires"));
        assert!(err(&format!("{FULL}native_format_version = \"1\"\n"))
            .mentions("native_format_version"));
    }

    #[test]
    fn parses_a_full_mod() {
        let m = Manifest::parse(FULL).unwrap();
        assert_eq!(m.id, "ember-age");
        assert_eq!(m.version, Version::new(1, 2, 0));
        assert_eq!(m.kind, Kind::Full);
        assert!(m.allow_derived);
        assert_eq!(m.approved_addons, ["ember-ui"]);
        assert!(m.base.is_none());
    }

    #[test]
    fn parses_an_addon_and_a_derived_mod() {
        let a = Manifest::parse(ADDON).unwrap();
        assert_eq!(a.kind, Kind::Addon);
        assert_eq!(a.attaches_to.as_deref(), Some("ember-age"));
        assert!(!a.screen_only);
        let derived = format!("{FULL}\n[base]\nid = \"other-mod\"\nversion = \"^2.0.0\"\n");
        let derived = replace(&derived, "ember-age", "ember-two");
        let d = Manifest::parse(&derived).unwrap();
        assert_eq!(d.base.unwrap().id, "other-mod");
    }

    #[test]
    fn optional_keys_default_off() {
        let min = r#"
id = "tiny-mod"
name = "Tiny"
version = "0.0.1"
author = "A"
description = ""
api = 1
kind = "full"
"#;
        let m = Manifest::parse(min).unwrap();
        assert!(!m.allow_derived && m.approved_addons.is_empty() && m.base.is_none());
    }

    #[test]
    fn round_trips_through_text() {
        for src in [FULL, ADDON] {
            let m = Manifest::parse(src).unwrap();
            assert_eq!(Manifest::parse(&m.to_toml()).unwrap(), m);
        }
        let derived =
            format!("{FULL}\n[base]\nid = \"other-mod\"\nversion = \">=1.0.0, <2.0.0\"\n");
        let derived = replace(&derived, "ember-age\"\nname", "ember-two\"\nname");
        let m = Manifest::parse(&derived).unwrap();
        assert_eq!(Manifest::parse(&m.to_toml()).unwrap(), m);
    }

    #[test]
    fn names_the_field_for_each_required_key() {
        for key in [
            "id",
            "name",
            "version",
            "author",
            "description",
            "api",
            "kind",
        ] {
            let text: String = FULL
                .lines()
                .filter(|l| !l.starts_with(&format!("{key} =")))
                .collect::<Vec<_>>()
                .join("\n");
            let e = err(&text);
            assert!(e.mentions(key), "missing {key}: {e}");
            assert!(e.to_string().contains(&format!("`{key}`")), "{e}");
        }
    }

    #[test]
    fn rejects_bad_ids() {
        for bad in [
            "Ember",
            "ab",
            "ember age",
            "-ember",
            "ember-",
            "em--ber",
            "9lives",
            "d2gamma",
            "base",
            "ember_age",
        ] {
            let e = err(&replace(FULL, "ember-age", bad));
            assert!(e.mentions("id"), "{bad}: {e}");
        }
        let long = "a".repeat(41);
        assert!(err(&replace(FULL, "ember-age", &long)).mentions("id"));
    }

    #[test]
    fn rejects_bad_values() {
        assert!(
            err(&replace(FULL, "version = \"1.2.0\"", "version = \"1.2\"")).mentions("version")
        );
        assert!(err(&replace(FULL, "api = 1", "api = 0")).mentions("api"));
        assert!(err(&replace(FULL, "api = 1", "api = -3")).mentions("api"));
        assert!(err(&replace(FULL, "api = 1", "api = \"one\"")).mentions("api"));
        assert!(err(&replace(FULL, "api = 1", "api = 1.5")).mentions("api"));
        assert!(err(&replace(FULL, "kind = \"full\"", "kind = \"mod\"")).mentions("kind"));
        assert!(err(&replace(FULL, "name = \"Ember Age\"", "name = \"  \"")).mentions("name"));
        assert!(err(&replace(FULL, "name = \"Ember Age\"", "name = 5")).mentions("name"));
        assert!(err(&replace(
            FULL,
            "allow_derived = true",
            "allow_derived = \"yes\""
        ))
        .mentions("allow_derived"));
        assert!(err(&replace(FULL, "[\"ember-ui\"]", "[\"Ember UI\"]")).mentions("approved_addons"));
        assert!(err(&replace(FULL, "[\"ember-ui\"]", "[\"a-b\", 3]")).mentions("approved_addons"));
        assert!(err(&replace(
            FULL,
            "[\"ember-ui\"]",
            "[\"ember-ui\", \"ember-ui\"]"
        ))
        .mentions("approved_addons"));
        assert!(
            err(&replace(FULL, "[\"ember-ui\"]", "[\"ember-age\"]")).mentions("approved_addons")
        );
    }

    #[test]
    fn rejects_unknown_keys_and_bad_toml() {
        let e = err(&format!("{FULL}\nflavour = \"spicy\"\n"));
        assert!(e.mentions("flavour"), "{e}");
        let e = err("id = ");
        assert!(e.mentions(""), "{e}");
        assert!(e.to_string().contains("not valid TOML"));
    }

    #[test]
    fn rejects_bad_base() {
        let with = |b: &str| format!("{FULL}\n[base]\n{b}\n");
        let id_only = err(&with("id = \"other-mod\""));
        assert!(id_only.mentions("base.version"), "{id_only}");
        assert!(err(&with("id = \"Other\"\nversion = \"^1.0.0\"")).mentions("base.id"));
        assert!(err(&with("id = \"other-mod\"\nversion = \"1.0\"")).mentions("base.version"));
        assert!(err(&with("id = \"ember-age\"\nversion = \"^1.0.0\"")).mentions("base.id"));
        assert!(
            err(&with("id = \"other-mod\"\nversion = \"^1.0.0\"\nextra = 1"))
                .mentions("base.extra")
        );
        assert!(err(&replace(FULL, "api = 1", "base = 3\napi = 1")).mentions("base"));
    }

    #[test]
    fn keys_must_match_the_kind() {
        // Full mods cannot use add-on keys.
        assert!(err(&format!("{FULL}\nfor = \"d2gamma\"\n")).mentions("for"));
        assert!(err(&format!("{FULL}\nscreen_only = true\n")).mentions("screen_only"));
        // Add-ons cannot use full-mod keys, and must say what they attach to.
        assert!(err(&format!("{ADDON}\nallow_derived = true\n")).mentions("allow_derived"));
        assert!(err(&format!("{ADDON}\napproved_addons = []\n")).mentions("approved_addons"));
        assert!(err(&format!(
            "{ADDON}\n[base]\nid = \"x-mod\"\nversion = \"1.0.0\"\n"
        ))
        .mentions("base"));
        let no_for = replace(ADDON, "for = \"ember-age\"\n", "");
        assert!(err(&no_for).mentions("for"));
        // An add-on for plain D2Gamma must be screen-only.
        let plain = replace(ADDON, "for = \"ember-age\"", "for = \"d2gamma\"");
        assert!(err(&plain).mentions("screen_only"));
        assert!(err(&format!("{plain}\nscreen_only = false\n")).mentions("screen_only"));
        let ok = Manifest::parse(&format!("{plain}\nscreen_only = true\n")).unwrap();
        assert!(ok.screen_only);
        // Attaching to itself.
        assert!(err(&replace(ADDON, "for = \"ember-age\"", "for = \"ember-ui\"")).mentions("for"));
    }

    #[test]
    fn reports_all_problems_together() {
        let e = err("id = \"X\"\nname = 3\nversion = \"nope\"");
        for f in [
            "id",
            "name",
            "version",
            "author",
            "description",
            "api",
            "kind",
        ] {
            assert!(e.mentions(f), "{f} in {e}");
        }
    }

    #[test]
    fn derivation_needs_permission_and_version() {
        let base = Manifest::parse(FULL).unwrap();
        let derived = |range: &str| {
            let t = format!(
                "id = \"ember-two\"\nname = \"Two\"\nversion = \"1.0.0\"\nauthor = \"B\"\ndescription = \"\"\napi = 1\nkind = \"full\"\n[base]\nid = \"ember-age\"\nversion = \"{range}\"\n"
            );
            Manifest::parse(&t).unwrap()
        };
        assert!(derived("^1.0.0").check_builds_on(&base).is_ok());
        let e = derived("^2.0.0").check_builds_on(&base).unwrap_err();
        assert!(e.contains("needs") && e.contains("1.2.0"), "{e}");
        let closed = Manifest::parse(&replace(
            FULL,
            "allow_derived = true",
            "allow_derived = false",
        ))
        .unwrap();
        let e = derived("^1.0.0").check_builds_on(&closed).unwrap_err();
        assert!(e.contains("does not allow"), "{e}");
        let other = Manifest::parse(&replace(FULL, "ember-age", "someone-else")).unwrap();
        assert!(derived("^1.0.0").check_builds_on(&other).is_err());
    }

    #[test]
    fn addons_need_the_makers_approval() {
        let target = Manifest::parse(FULL).unwrap();
        let addon = Manifest::parse(ADDON).unwrap();
        assert!(addon.check_addon_for(Some(&target)).is_ok());
        let unapproved = Manifest::parse(&replace(ADDON, "ember-ui", "ember-extra")).unwrap();
        let e = unapproved.check_addon_for(Some(&target)).unwrap_err();
        assert!(e.contains("has not approved"), "{e}");
        assert!(addon.check_addon_for(None).is_err());
        let screen = Manifest::parse(&replace(
            ADDON,
            "for = \"ember-age\"",
            "for = \"d2gamma\"\nscreen_only = true",
        ))
        .unwrap();
        assert!(screen.check_addon_for(None).is_ok());
        assert!(screen.check_addon_for(Some(&target)).is_err());
        assert!(target.check_addon_for(None).is_err());
    }
}

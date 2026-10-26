//! The saved layouts of floating panels: one per realm, account and scene
//! (the lobby and a game apart), kept on the client in `layouts.toml` in
//! the config folder (beside `settings.toml`), never on the realm. Each
//! panel's rect is stored anchored to its nearest canvas corner
//! ([`Anchored`]), so it survives the lobby's 800 × 600 and a game's view.
//!
//! ```toml
//! version = 1
//!
//! [[layout]]
//! realm = "d2gamma-eu"
//! account = "nugwa"
//! scene = "lobby"
//! locked = false
//!
//! [[layout.panel]]
//! id = "chat"
//! corner = "top-left"
//! dx = 48
//! dy = 179
//! w = 352
//! h = 326
//! ```

use std::path::{Path, PathBuf};

use super::geom::{Anchored, Corner};

/// The file's version.
pub const LAYOUTS_VERSION: i64 = 1;

/// The file in the config folder.
pub fn layouts_path(config_dir: &Path) -> PathBuf {
    config_dir.join("layouts.toml")
}

/// Where the panels are.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scene {
    Lobby,
    Game,
}

impl Scene {
    pub fn key(self) -> &'static str {
        match self {
            Scene::Lobby => "lobby",
            Scene::Game => "game",
        }
    }

    pub fn from_key(s: &str) -> Option<Self> {
        [Scene::Lobby, Scene::Game]
            .into_iter()
            .find(|c| c.key() == s)
    }
}

/// Whose layout: the realm (its server id), the account (any case) and
/// the scene.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LayoutKey {
    pub realm: String,
    pub account: String,
    pub scene: Scene,
}

impl LayoutKey {
    /// The key; the account is kept in lower case (account names are
    /// one account in any case).
    pub fn new(realm: &str, account: &str, scene: Scene) -> Self {
        Self {
            realm: realm.to_owned(),
            account: account.to_lowercase(),
            scene,
        }
    }
}

/// One saved layout: the lock and each moved panel's rect by panel id.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SavedLayout {
    pub locked: bool,
    pub panels: Vec<(String, Anchored)>,
}

impl SavedLayout {
    pub fn panel(&self, id: &str) -> Option<&Anchored> {
        self.panels.iter().find(|(p, _)| p == id).map(|(_, a)| a)
    }
}

/// Every saved layout of the client.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LayoutStore {
    layouts: Vec<(LayoutKey, SavedLayout)>,
}

impl LayoutStore {
    pub fn get(&self, key: &LayoutKey) -> Option<&SavedLayout> {
        self.layouts.iter().find(|(k, _)| k == key).map(|(_, l)| l)
    }

    /// Stores `layout` under `key` (replacing the one there).
    pub fn set(&mut self, key: LayoutKey, layout: SavedLayout) {
        match self.layouts.iter_mut().find(|(k, _)| *k == key) {
            Some((_, l)) => *l = layout,
            None => self.layouts.push((key, layout)),
        }
    }

    /// Forgets the layout of `key` (the host's classic layout again).
    pub fn remove(&mut self, key: &LayoutKey) {
        self.layouts.retain(|(k, _)| k != key);
    }

    pub fn len(&self) -> usize {
        self.layouts.len()
    }

    pub fn is_empty(&self) -> bool {
        self.layouts.is_empty()
    }

    /// The file's text.
    pub fn to_text(&self) -> String {
        let mut out = format!("version = {LAYOUTS_VERSION}\n");
        for (k, l) in &self.layouts {
            out.push_str("\n[[layout]]\n");
            out.push_str(&format!("realm = {}\n", toml_str(&k.realm)));
            out.push_str(&format!("account = {}\n", toml_str(&k.account)));
            out.push_str(&format!("scene = \"{}\"\n", k.scene.key()));
            out.push_str(&format!("locked = {}\n", l.locked));
            for (id, a) in &l.panels {
                out.push_str("\n[[layout.panel]]\n");
                out.push_str(&format!("id = {}\n", toml_str(id)));
                out.push_str(&format!("corner = \"{}\"\n", a.corner.key()));
                out.push_str(&format!(
                    "dx = {}\ndy = {}\nw = {}\nh = {}\n",
                    a.dx, a.dy, a.w, a.h
                ));
            }
        }
        out
    }

    /// Parses the file's text. A missing key, a wrong type or an unknown
    /// corner or scene fails the whole file.
    pub fn parse(text: &str) -> Result<Self, String> {
        let doc: toml_edit::DocumentMut = text.parse().map_err(|e| format!("{e}"))?;
        let version = doc
            .get("version")
            .and_then(toml_edit::Item::as_integer)
            .ok_or("no version")?;
        if version != LAYOUTS_VERSION {
            return Err(format!("version {version} is not {LAYOUTS_VERSION}"));
        }
        let mut store = Self::default();
        let Some(item) = doc.get("layout") else {
            return Ok(store);
        };
        let tables = item
            .as_array_of_tables()
            .ok_or("`layout` is not [[layout]]")?;
        for t in tables {
            let scene = str_key(t, "scene")?;
            let key = LayoutKey::new(
                &str_key(t, "realm")?,
                &str_key(t, "account")?,
                Scene::from_key(&scene).ok_or_else(|| format!("unknown scene `{scene}`"))?,
            );
            let locked = t
                .get("locked")
                .map(|v| v.as_bool().ok_or("`locked` is not true or false"))
                .transpose()?
                .unwrap_or(false);
            let mut layout = SavedLayout {
                locked,
                panels: Vec::new(),
            };
            if let Some(panels) = t.get("panel") {
                let panels = panels
                    .as_array_of_tables()
                    .ok_or("`panel` is not [[layout.panel]]")?;
                for p in panels {
                    let corner = str_key(p, "corner")?;
                    let anchored = Anchored {
                        corner: Corner::from_key(&corner)
                            .ok_or_else(|| format!("unknown corner `{corner}`"))?,
                        dx: int_key(p, "dx")?,
                        dy: int_key(p, "dy")?,
                        w: size_key(p, "w")?,
                        h: size_key(p, "h")?,
                    };
                    layout.panels.push((str_key(p, "id")?, anchored));
                }
            }
            store.set(key, layout);
        }
        Ok(store)
    }

    /// The layouts in `config_dir`; none when there is no file. A damaged
    /// file is an error (the host keeps the classic layouts and says so).
    pub fn load(config_dir: &Path) -> Result<Self, String> {
        let path = layouts_path(config_dir);
        match std::fs::read_to_string(&path) {
            Ok(text) => Self::parse(&text).map_err(|e| format!("{}: {e}", path.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(format!("{}: {e}", path.display())),
        }
    }

    /// Writes the layouts to `config_dir` through a temp file and a rename,
    /// so a crash keeps the old file.
    pub fn save(&self, config_dir: &Path) -> Result<(), String> {
        let path = layouts_path(config_dir);
        std::fs::create_dir_all(config_dir)
            .map_err(|e| format!("{}: {e}", config_dir.display()))?;
        let tmp = path.with_extension("toml.tmp");
        std::fs::write(&tmp, self.to_text()).map_err(|e| format!("{}: {e}", tmp.display()))?;
        std::fs::rename(&tmp, &path).map_err(|e| format!("{}: {e}", path.display()))
    }
}

/// `s` as a TOML basic string.
fn toml_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if c.is_control() => out.push_str(&format!("\\u{:04X}", u32::from(c))),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn str_key(t: &toml_edit::Table, key: &str) -> Result<String, String> {
    t.get(key)
        .ok_or_else(|| format!("no `{key}`"))?
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| format!("`{key}` is not a string"))
}

fn int_key(t: &toml_edit::Table, key: &str) -> Result<i32, String> {
    let v = t
        .get(key)
        .ok_or_else(|| format!("no `{key}`"))?
        .as_integer()
        .ok_or_else(|| format!("`{key}` is not a number"))?;
    i32::try_from(v).map_err(|_| format!("`{key}` {v} is out of range"))
}

fn size_key(t: &toml_edit::Table, key: &str) -> Result<u16, String> {
    let v = int_key(t, key)?;
    u16::try_from(v).map_err(|_| format!("`{key}` {v} is not a size"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> LayoutStore {
        let mut s = LayoutStore::default();
        s.set(
            LayoutKey::new("d2gamma-eu", "Nugwa", Scene::Lobby),
            SavedLayout {
                locked: true,
                panels: vec![
                    (
                        "chat".into(),
                        Anchored {
                            corner: Corner::TopLeft,
                            dx: 48,
                            dy: 179,
                            w: 352,
                            h: 326,
                        },
                    ),
                    (
                        "right".into(),
                        Anchored {
                            corner: Corner::BottomRight,
                            dx: 34,
                            dy: 96,
                            w: 320,
                            h: 192,
                        },
                    ),
                ],
            },
        );
        // The same realm and account in a game; an odd realm id.
        s.set(
            LayoutKey::new("d2gamma-eu", "nugwa", Scene::Game),
            SavedLayout {
                locked: false,
                panels: vec![(
                    "chat".into(),
                    Anchored {
                        corner: Corner::BottomLeft,
                        dx: 0,
                        dy: 120,
                        w: 420,
                        h: 200,
                    },
                )],
            },
        );
        s.set(
            LayoutKey::new("my \"home\" \\ realm\t", "x", Scene::Lobby),
            SavedLayout::default(),
        );
        s
    }

    #[test]
    fn store_round_trip() {
        let s = sample();
        let text = s.to_text();
        assert_eq!(LayoutStore::parse(&text), Ok(s.clone()), "{text}");
        // Lobby and game apart, the account in any case.
        let lobby = s
            .get(&LayoutKey::new("d2gamma-eu", "NUGWA", Scene::Lobby))
            .unwrap();
        assert!(lobby.locked);
        assert_eq!(lobby.panel("right").unwrap().corner, Corner::BottomRight);
        let game = s
            .get(&LayoutKey::new("d2gamma-eu", "nugwa", Scene::Game))
            .unwrap();
        assert_eq!(game.panel("chat").unwrap().w, 420);
        assert_eq!(s.get(&LayoutKey::new("other", "nugwa", Scene::Lobby)), None);

        // Through the file.
        let dir = std::env::temp_dir().join(format!("d2rust-layouts-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(LayoutStore::load(&dir), Ok(LayoutStore::default()));
        s.save(&dir).unwrap();
        assert_eq!(LayoutStore::load(&dir), Ok(s));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn set_replaces_and_remove_forgets() {
        let mut s = sample();
        let key = LayoutKey::new("d2gamma-eu", "nugwa", Scene::Lobby);
        s.set(key.clone(), SavedLayout::default());
        assert_eq!(s.len(), 3);
        assert_eq!(s.get(&key), Some(&SavedLayout::default()));
        s.remove(&key);
        assert_eq!(s.len(), 2);
        assert_eq!(s.get(&key), None);
    }

    #[test]
    fn a_damaged_file_is_an_error() {
        assert_eq!(
            LayoutStore::parse("version = 1\n"),
            Ok(LayoutStore::default())
        );
        assert!(LayoutStore::parse("").is_err());
        assert!(LayoutStore::parse("version = 2\n").is_err());
        let base = "version = 1\n[[layout]]\nrealm = \"r\"\naccount = \"a\"\nscene = \"lobby\"\n";
        assert!(LayoutStore::parse(base).is_ok());
        assert!(LayoutStore::parse(&base.replace("lobby", "hall")).is_err());
        let panel = "[[layout.panel]]\nid = \"chat\"\ncorner = \"top-left\"\ndx = 1\ndy = 2\nw = 3\nh = 4\n";
        assert!(LayoutStore::parse(&format!("{base}{panel}")).is_ok());
        assert!(
            LayoutStore::parse(&format!("{base}{}", panel.replace("top-left", "middle"))).is_err()
        );
        assert!(
            LayoutStore::parse(&format!("{base}{}", panel.replace("w = 3", "w = -3"))).is_err()
        );
        assert!(LayoutStore::parse(&format!("{base}{}", panel.replace("dx = 1\n", ""))).is_err());
    }
}

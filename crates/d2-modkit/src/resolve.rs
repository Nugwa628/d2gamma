//! Finding the mod to run: `--mod <id>` and `--mod-dir <folder>`.
//!
//! Installed mods live in one folder, one subfolder per mod named by its
//! `id` (`Documents\D2Gamma\mods\<id>\`). [`resolve`] turns what the player
//! (or Mod Studio) asked for into the ordered list of mod folders to stack
//! over the base data, checking on the way, in plain words:
//!
//! - the folder holds a mod-kit `mod.toml` that parses;
//! - the mod's modding API version is one this build runs
//!   ([`crate::api::check_api_now`]);
//! - it is a full mod (an add-on cannot be started on its own);
//! - if it builds on another mod, that mod is installed, allows derived
//!   mods and is in the version range ([`Manifest::check_builds_on`]); the
//!   base comes first in the stack, its own base before it;
//! - every mod it `requires` is earlier in the stack.
//!
//! Loading the files is the layer stack's job; this module only decides
//! which folders, in which order.

use std::fs;
use std::path::{Path, PathBuf};

use crate::api::{check_api, ApiCheck, Date};
use crate::error::ModError;
use crate::layout::{
    AI_FILE, CROWD_CONTROL_FILE, DIFFICULTIES_FILE, FILES_DIR, QUESTS_FILE, RULES_FILE,
    SKILLS_FILE, STATES_FILE, STATS_FILE, TRIGGERS_FILE,
};
use crate::manifest::{check_id, Kind, Manifest, MANIFEST_FILE};

/// How deep a chain of "builds on" may go.
const MAX_CHAIN: usize = 8;

/// What the player asked to run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModSpec {
    /// An installed mod, by id.
    Id(String),
    /// Any mod folder (Mod Studio's play button, a mod being tested).
    Dir(PathBuf),
}

impl ModSpec {
    /// `--mod <arg>`: a valid mod id names an installed mod, anything else
    /// is taken as a folder.
    pub fn from_mod_arg(arg: &str) -> ModSpec {
        if check_id(arg).is_ok() {
            ModSpec::Id(arg.to_owned())
        } else {
            ModSpec::Dir(PathBuf::from(arg))
        }
    }

    /// `--mod-dir <path>`: always a folder.
    pub fn from_dir_arg(arg: &str) -> ModSpec {
        ModSpec::Dir(PathBuf::from(arg))
    }
}

/// A mod folder that passed the checks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedMod {
    pub manifest: Manifest,
    pub dir: PathBuf,
}

/// The mods to stack, bases first; the last one is the mod the player
/// picked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModStack {
    pub mods: Vec<LoadedMod>,
}

impl ModStack {
    /// The mod the player picked.
    pub fn picked(&self) -> &LoadedMod {
        self.mods
            .iter()
            .rev()
            .find(|m| m.manifest.kind == Kind::Full)
            .or_else(|| self.mods.last())
            .expect("a stack holds at least one mod")
    }

    /// The layer list for the layer stack: the absolute folder of each mod,
    /// bases first.
    pub fn layer_entries(&self) -> Vec<String> {
        self.mods
            .iter()
            .map(|m| m.dir.display().to_string())
            .collect()
    }

    /// The mod's `files/` folder of the picked mod (where its icon, intro
    /// and rules live).
    pub fn picked_files(&self) -> PathBuf {
        self.picked().dir.join(FILES_DIR)
    }

    /// The global rules file (`files/rules.toml`) the stack plays under:
    /// the last mod that has one, as with every other file a later layer
    /// replaces. `None` when no mod in the stack has one.
    pub fn rules_file(&self) -> Result<Option<RulesFile>, ModError> {
        self.last_file(RULES_FILE)
    }

    /// The states and stacks file (`files/states.toml`) the stack plays
    /// under, found like [`ModStack::rules_file`]: the last mod that has
    /// one replaces the others whole.
    pub fn states_file(&self) -> Result<Option<RulesFile>, ModError> {
        self.last_file(STATES_FILE)
    }

    /// The building-block skills file (`files/skills.toml`) the stack plays
    /// under, found like [`ModStack::rules_file`].
    pub fn skills_file(&self) -> Result<Option<RulesFile>, ModError> {
        self.last_file(SKILLS_FILE)
    }

    /// The triggers file (`files/triggers.toml`) the stack plays under,
    /// found like [`ModStack::rules_file`].
    pub fn triggers_file(&self) -> Result<Option<RulesFile>, ModError> {
        self.last_file(TRIGGERS_FILE)
    }

    /// The declared crowd control file (`files/crowd_control.toml`) the stack
    /// plays under, found like [`ModStack::rules_file`].
    pub fn crowd_control_file(&self) -> Result<Option<RulesFile>, ModError> {
        self.last_file(CROWD_CONTROL_FILE)
    }

    /// The monster behaviour graphs file (`files/ai.toml`) the stack plays
    /// under, found like [`ModStack::rules_file`].
    pub fn ai_file(&self) -> Result<Option<RulesFile>, ModError> {
        self.last_file(AI_FILE)
    }

    /// The quests file (`files/quests.toml`) the stack plays under, found
    /// like [`ModStack::rules_file`].
    pub fn quests_file(&self) -> Result<Option<RulesFile>, ModError> {
        self.last_file(QUESTS_FILE)
    }

    /// The last layer's complete act boundaries, towns and travel.
    pub fn acts_file(&self) -> Result<Option<RulesFile>, ModError> {
        self.last_file(crate::layout::ACTS_FILE)
    }

    /// The last layer's complete difficulty names and count.
    pub fn difficulties_file(&self) -> Result<Option<RulesFile>, ModError> {
        self.last_file(DIFFICULTIES_FILE)
    }

    /// The last layer's complete named stat registry.
    pub fn stats_file(&self) -> Result<Option<RulesFile>, ModError> {
        self.last_file(STATS_FILE)
    }

    fn last_file(&self, relative: &str) -> Result<Option<RulesFile>, ModError> {
        for m in self.mods.iter().rev() {
            let path = m.dir.join(relative);
            match fs::read_to_string(&path) {
                Ok(text) => return Ok(Some(RulesFile { path, text })),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(ModError::io(path.display(), e)),
            }
        }
        Ok(None)
    }
}

/// The text of a mod's rules or states file and where it came from (for the
/// messages that name it).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RulesFile {
    pub path: PathBuf,
    pub text: String,
}

fn load(msg: impl Into<String>) -> ModError {
    ModError::Load(msg.into())
}

/// Reads the mod folder `dir` as a mod-kit mod and checks its API version.
fn open(dir: &Path, today: Date) -> Result<LoadedMod, ModError> {
    if !dir.is_dir() {
        return Err(load(format!("there is no mod folder at {}", dir.display())));
    }
    let manifest_path = dir.join(MANIFEST_FILE);
    let text = match fs::read_to_string(&manifest_path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(load(format!(
                "{} has no {MANIFEST_FILE}, so it is not a mod",
                dir.display()
            )))
        }
        Err(e) => return Err(ModError::io(manifest_path.display(), e)),
    };
    let manifest = match crate::manifest::parse_mod_toml(&text)? {
        crate::manifest::ModToml::Kit(m) => *m,
        crate::manifest::ModToml::Legacy(_) => {
            return Err(load(format!(
                "{} is a layer-only mod with no `api` in its {MANIFEST_FILE}, so D2Gamma cannot start it by itself",
                dir.display()
            )))
        }
    };
    match check_api(manifest.api, today) {
        ApiCheck::Ok => {}
        ApiCheck::NeedsUpdate(msg) | ApiCheck::TooNew(msg) => {
            return Err(load(format!("{}: {msg}", manifest.name)))
        }
    }
    Ok(LoadedMod {
        manifest,
        dir: dir.to_path_buf(),
    })
}

/// Resolves `spec` against the installed mods in `mods_dir`, as of `today`.
pub fn resolve_at(spec: &ModSpec, mods_dir: &Path, today: Date) -> Result<ModStack, ModError> {
    resolve_with(spec, mods_dir, today, open)
}

/// Resolves folders or containers through a caller-provided manifest reader.
/// The reader must verify a container's integrity before returning its manifest.
pub fn resolve_with(
    spec: &ModSpec,
    mods_dir: &Path,
    today: Date,
    open: impl Fn(&Path, Date) -> Result<LoadedMod, ModError>,
) -> Result<ModStack, ModError> {
    let picked = match spec {
        ModSpec::Id(id) => {
            let dir = installed_path(mods_dir, id);
            if !dir.exists() {
                return Err(load(format!(
                    "no mod \"{id}\" is installed (looked in {}); to run a mod from another folder use --mod-dir <folder>",
                    mods_dir.display()
                )));
            }
            let m = open(&dir, today)?;
            if m.manifest.id != *id {
                return Err(load(format!(
                    "the folder {} holds the mod \"{}\", not \"{id}\"; a mod's folder is named by its id",
                    dir.display(),
                    m.manifest.id
                )));
            }
            m
        }
        ModSpec::Dir(dir) => open(dir, today)?,
    };
    if picked.manifest.kind != Kind::Full {
        return Err(load(format!(
            "{} is an add-on; it runs with the mod it is made for, not by itself",
            picked.manifest.name
        )));
    }

    // Walk down the "builds on" chain, then put the bases first.
    let mut chain = vec![picked];
    while let Some(base_ref) = chain.last().and_then(|m| m.manifest.base.clone()) {
        if chain.len() >= MAX_CHAIN {
            return Err(load(format!(
                "mods build on each other more than {MAX_CHAIN} deep; check the `base` keys"
            )));
        }
        if chain.iter().any(|m| m.manifest.id == base_ref.id) {
            return Err(load(format!(
                "\"{}\" builds on itself through the mods it builds on",
                base_ref.id
            )));
        }
        let child = chain.last().expect("non-empty").manifest.clone();
        let dir = installed_path(mods_dir, &base_ref.id);
        if !dir.exists() {
            return Err(load(format!(
                "{} builds on the mod \"{}\" ({}), which is not installed",
                child.name, base_ref.id, base_ref.version
            )));
        }
        let base = open(&dir, today)?;
        child.check_builds_on(&base.manifest).map_err(load)?;
        chain.push(base);
    }
    chain.reverse();

    for (i, m) in chain.iter().enumerate() {
        if let Some(r) = m
            .manifest
            .requires
            .iter()
            .find(|r| !chain[..i].iter().any(|e| &e.manifest.id == *r))
        {
            return Err(load(format!(
                "{} needs the mod \"{r}\" loaded before it, and it is not part of this stack",
                m.manifest.name
            )));
        }
    }
    let target = chain.last().expect("picked full mod").manifest.id.clone();
    for id in crate::addons::enabled(mods_dir, &target).map_err(load)? {
        let addon = open(&installed_path(mods_dir, &id), today)?;
        if addon.manifest.id != id || addon.manifest.kind != Kind::Addon {
            return Err(load(format!("{id} is not the requested add-on")));
        }
        chain.push(addon);
    }
    crate::addons::check_stack(&chain.iter().map(|m| &m.manifest).collect::<Vec<_>>())
        .map_err(load)?;
    Ok(ModStack { mods: chain })
}

pub fn installed_path(mods_dir: &Path, id: &str) -> PathBuf {
    let dir = mods_dir.join(id);
    let pack = mods_dir.join(format!("{id}.d2pack"));
    // A pack's saves/settings also live in mods/<id>; that directory is
    // editable mod content only when it contains its own manifest.
    if dir.join(MANIFEST_FILE).is_file() || !pack.is_file() {
        dir
    } else {
        pack
    }
}

/// Folder manifest reader for container-aware resolvers.
pub fn open_folder(dir: &Path, today: Date) -> Result<LoadedMod, ModError> {
    open(dir, today)
}

/// [`resolve_at`] as of today.
pub fn resolve(spec: &ModSpec, mods_dir: &Path) -> Result<ModStack, ModError> {
    resolve_at(spec, mods_dir, Date::today())
}

/// One entry of the installed-mods list.
#[derive(Debug, Clone)]
pub struct Installed {
    /// The folder name (the mod's id when the mod is well formed).
    pub folder: String,
    pub dir: PathBuf,
    /// The manifest, or why the mod cannot be listed as usable.
    pub manifest: Result<Manifest, ModError>,
}

/// Every mod folder in `mods_dir`, sorted by folder name. A broken mod is
/// listed with its error, so a mods screen can say why it cannot be picked.
pub fn list_installed(mods_dir: &Path) -> Vec<Installed> {
    let Ok(read) = fs::read_dir(mods_dir) else {
        return Vec::new();
    };
    let mut out: Vec<Installed> = read
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .filter_map(|e| {
            let folder = e.file_name().to_str()?.to_owned();
            if folder.starts_with('.') {
                return None;
            }
            let dir = e.path();
            let manifest = fs::read_to_string(dir.join(MANIFEST_FILE))
                .map_err(|er| ModError::io(dir.join(MANIFEST_FILE).display(), er))
                .and_then(|t| Manifest::parse(&t).map_err(ModError::from));
            Some(Installed {
                folder,
                dir,
                manifest,
            })
        })
        .collect();
    out.sort_by(|a, b| a.folder.cmp(&b.folder));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::CURRENT_API;
    use crate::testutil::temp;

    fn today() -> Date {
        Date::new(2026, 11, 1)
    }

    fn manifest(id: &str, extra: &str) -> String {
        format!(
            "id = \"{id}\"\nname = \"Mod {id}\"\nversion = \"1.0.0\"\nauthor = \"A\"\ndescription = \"\"\napi = {CURRENT_API}\nkind = \"full\"\n{extra}"
        )
    }

    fn install(mods: &Path, id: &str, text: &str) -> PathBuf {
        let d = mods.join(id);
        fs::create_dir_all(d.join("files")).unwrap();
        fs::write(d.join("mod.toml"), text).unwrap();
        d
    }

    fn err(spec: &ModSpec, mods: &Path) -> String {
        resolve_at(spec, mods, today()).unwrap_err().to_string()
    }

    #[test]
    fn the_last_mod_with_a_rules_file_wins() {
        let mods = temp("rules-file");
        let base = install(
            &mods,
            "base-mod",
            &manifest("base-mod", "allow_derived = true\n"),
        );
        let derived = install(
            &mods,
            "derived-mod",
            &manifest(
                "derived-mod",
                "[base]\nid = \"base-mod\"\nversion = \"^1.0.0\"\n",
            ),
        );
        let stack = |id: &str| resolve_at(&ModSpec::Id(id.into()), &mods, today());
        // No rules file anywhere: nothing to read.
        assert_eq!(stack("derived-mod").unwrap().rules_file().unwrap(), None);
        fs::write(base.join("files/rules.toml"), "max_block_chance = 60\n").unwrap();
        let found = stack("derived-mod").unwrap().rules_file().unwrap().unwrap();
        assert_eq!(found.path, base.join("files/rules.toml"));
        assert_eq!(found.text, "max_block_chance = 60\n");
        // The derived mod's own file replaces the base's.
        fs::write(derived.join("files/rules.toml"), "max_block_chance = 40\n").unwrap();
        let found = stack("derived-mod").unwrap().rules_file().unwrap().unwrap();
        assert_eq!(found.path, derived.join("files/rules.toml"));
        // The base alone still sees its own.
        let found = stack("base-mod").unwrap().rules_file().unwrap().unwrap();
        assert_eq!(found.text, "max_block_chance = 60\n");
    }

    #[test]
    fn the_last_mod_with_a_states_file_wins() {
        let mods = temp("states-file");
        let base = install(
            &mods,
            "base-mod",
            &manifest(
                "base-mod",
                "allow_derived = true
",
            ),
        );
        let derived = install(
            &mods,
            "derived-mod",
            &manifest(
                "derived-mod",
                "[base]
id = \"base-mod\"
version = \"^1.0.0\"
",
            ),
        );
        let stack = |id: &str| resolve_at(&ModSpec::Id(id.into()), &mods, today()).unwrap();
        assert_eq!(stack("derived-mod").states_file().unwrap(), None);
        fs::write(
            base.join("files/states.toml"),
            "version = 1
",
        )
        .unwrap();
        let found = stack("derived-mod").states_file().unwrap().unwrap();
        assert_eq!(found.path, base.join("files/states.toml"));
        // Its own file replaces the base's whole; the rules file is separate.
        fs::write(
            derived.join("files/states.toml"),
            "# mine
",
        )
        .unwrap();
        let found = stack("derived-mod").states_file().unwrap().unwrap();
        assert_eq!(
            found.text,
            "# mine
"
        );
        assert_eq!(stack("derived-mod").rules_file().unwrap(), None);
        // The triggers file is found the same way.
        assert_eq!(stack("derived-mod").triggers_file().unwrap(), None);
        fs::write(base.join("files/triggers.toml"), "version = 1").unwrap();
        let found = stack("derived-mod").triggers_file().unwrap().unwrap();
        assert_eq!(found.path, base.join("files/triggers.toml"));
        // So is the declared crowd control file.
        assert_eq!(stack("derived-mod").crowd_control_file().unwrap(), None);
        fs::write(base.join("files/crowd_control.toml"), "version = 1").unwrap();
        let found = stack("derived-mod").crowd_control_file().unwrap().unwrap();
        assert_eq!(found.path, base.join("files/crowd_control.toml"));
        // And the monster behaviour graphs.
        assert_eq!(stack("derived-mod").ai_file().unwrap(), None);
        fs::write(base.join("files/ai.toml"), "version = 1").unwrap();
        let found = stack("derived-mod").ai_file().unwrap().unwrap();
        assert_eq!(found.path, base.join("files/ai.toml"));
        // And the quests file.
        assert_eq!(stack("derived-mod").quests_file().unwrap(), None);
        fs::write(base.join("files/quests.toml"), "version = 1").unwrap();
        let found = stack("derived-mod").quests_file().unwrap().unwrap();
        assert_eq!(found.path, base.join("files/quests.toml"));
        assert_eq!(stack("derived-mod").stats_file().unwrap(), None);
        fs::write(base.join("files/stats.toml"), "# base").unwrap();
        assert_eq!(
            stack("derived-mod").stats_file().unwrap().unwrap().path,
            base.join("files/stats.toml")
        );
        fs::write(derived.join("files/stats.toml"), "# derived").unwrap();
        assert_eq!(
            stack("derived-mod").stats_file().unwrap().unwrap().text,
            "# derived"
        );
    }

    #[test]
    fn args_pick_an_id_or_a_folder() {
        assert_eq!(
            ModSpec::from_mod_arg("ember-age"),
            ModSpec::Id("ember-age".into())
        );
        assert!(matches!(ModSpec::from_mod_arg("./my mod"), ModSpec::Dir(_)));
        assert!(matches!(
            ModSpec::from_mod_arg("C:\\mods\\x"),
            ModSpec::Dir(_)
        ));
        assert!(matches!(
            ModSpec::from_dir_arg("ember-age"),
            ModSpec::Dir(_)
        ));
    }

    #[test]
    fn resolves_an_installed_mod_and_a_folder() {
        let mods = temp("resolve-one");
        let d = install(&mods, "ember-age", &manifest("ember-age", ""));
        let s = resolve_at(&ModSpec::Id("ember-age".into()), &mods, today()).unwrap();
        assert_eq!(s.mods.len(), 1);
        assert_eq!(s.picked().manifest.name, "Mod ember-age");
        assert_eq!(s.layer_entries(), [d.display().to_string()]);
        assert_eq!(s.picked_files(), d.join("files"));
        // Any folder, whatever it is called.
        let other = temp("resolve-folder").join("work in progress");
        fs::create_dir_all(&other).unwrap();
        fs::write(other.join("mod.toml"), manifest("wip-mod", "")).unwrap();
        let s = resolve_at(&ModSpec::Dir(other.clone()), &mods, today()).unwrap();
        assert_eq!(s.picked().manifest.id, "wip-mod");
    }

    #[test]
    fn says_what_is_wrong_in_plain_words() {
        let mods = temp("resolve-errors");
        let e = err(&ModSpec::Id("nope-mod".into()), &mods);
        assert!(
            e.contains("no mod \"nope-mod\" is installed") && e.contains("--mod-dir"),
            "{e}"
        );
        assert!(err(&ModSpec::Dir(mods.join("missing")), &mods).contains("no mod folder"));
        fs::create_dir_all(mods.join("empty-mod")).unwrap();
        assert!(err(&ModSpec::Id("empty-mod".into()), &mods).contains("not a mod"));
        // A manifest problem is named by field.
        install(
            &mods,
            "bad-mod",
            &manifest("bad-mod", "").replace("1.0.0", "x"),
        );
        assert!(err(&ModSpec::Id("bad-mod".into()), &mods).contains("`version`"));
        // The folder must be named by the id.
        install(&mods, "wrong-name", &manifest("right-name", ""));
        assert!(err(&ModSpec::Id("wrong-name".into()), &mods).contains("named by its id"));
        // Layer-only mods are not started by themselves.
        install(
            &mods,
            "old-mod",
            "name = \"o\"\nversion = \"1\"\nnative_format_version = 1\n",
        );
        assert!(err(&ModSpec::Id("old-mod".into()), &mods).contains("layer-only"));
    }

    #[test]
    fn api_versions_are_checked() {
        let mods = temp("resolve-api");
        let newer = manifest("new-mod", "").replace(
            &format!("api = {CURRENT_API}"),
            &format!("api = {}", CURRENT_API + 1),
        );
        install(&mods, "new-mod", &newer);
        let e = err(&ModSpec::Id("new-mod".into()), &mods);
        assert!(e.contains("Update D2Gamma"), "{e}");
    }

    #[test]
    fn add_ons_are_not_started_alone() {
        let mods = temp("resolve-addon");
        let text = manifest("an-addon", "for = \"d2gamma\"\nscreen_only = true\n")
            .replace("kind = \"full\"", "kind = \"addon\"");
        install(&mods, "an-addon", &text);
        assert!(err(&ModSpec::Id("an-addon".into()), &mods).contains("add-on"));
    }

    #[test]
    fn a_derived_mod_stacks_over_its_base() {
        let mods = temp("resolve-derived");
        let grand = install(
            &mods,
            "grand-mod",
            &manifest("grand-mod", "allow_derived = true\n"),
        );
        let base = install(
            &mods,
            "base-mod",
            &manifest(
                "base-mod",
                "allow_derived = true\n[base]\nid = \"grand-mod\"\nversion = \"^1.0.0\"\n",
            ),
        );
        let top = install(
            &mods,
            "top-mod",
            &manifest(
                "top-mod",
                "[base]\nid = \"base-mod\"\nversion = \">=1.0.0, <2.0.0\"\n",
            ),
        );
        let s = resolve_at(&ModSpec::Id("top-mod".into()), &mods, today()).unwrap();
        let dirs: Vec<PathBuf> = s.mods.iter().map(|m| m.dir.clone()).collect();
        assert_eq!(dirs, [grand, base, top]);
        assert_eq!(s.picked().manifest.id, "top-mod");
    }

    #[test]
    fn building_on_needs_permission_version_and_presence() {
        let mods = temp("resolve-base-rules");
        install(&mods, "closed-mod", &manifest("closed-mod", ""));
        install(
            &mods,
            "child-a",
            &manifest(
                "child-a",
                "[base]\nid = \"closed-mod\"\nversion = \"^1.0.0\"\n",
            ),
        );
        let e = err(&ModSpec::Id("child-a".into()), &mods);
        assert!(e.contains("does not allow"), "{e}");

        install(
            &mods,
            "open-mod",
            &manifest("open-mod", "allow_derived = true\n"),
        );
        install(
            &mods,
            "child-b",
            &manifest(
                "child-b",
                "[base]\nid = \"open-mod\"\nversion = \"^2.0.0\"\n",
            ),
        );
        let e = err(&ModSpec::Id("child-b".into()), &mods);
        assert!(e.contains("needs") && e.contains("1.0.0"), "{e}");

        install(
            &mods,
            "child-c",
            &manifest(
                "child-c",
                "[base]\nid = \"absent-mod\"\nversion = \"^1.0.0\"\n",
            ),
        );
        let e = err(&ModSpec::Id("child-c".into()), &mods);
        assert!(
            e.contains("absent-mod") && e.contains("not installed"),
            "{e}"
        );
    }

    #[test]
    fn a_loop_of_bases_is_refused() {
        let mods = temp("resolve-loop");
        install(
            &mods,
            "loop-a",
            &manifest(
                "loop-a",
                "allow_derived = true\n[base]\nid = \"loop-b\"\nversion = \"^1.0.0\"\n",
            ),
        );
        install(
            &mods,
            "loop-b",
            &manifest(
                "loop-b",
                "allow_derived = true\n[base]\nid = \"loop-a\"\nversion = \"^1.0.0\"\n",
            ),
        );
        assert!(err(&ModSpec::Id("loop-a".into()), &mods).contains("builds on itself"));
    }

    #[test]
    fn requires_must_be_earlier_in_the_stack() {
        let mods = temp("resolve-requires");
        install(
            &mods,
            "needy-mod",
            &manifest("needy-mod", "requires = [\"lib-mod\"]\n"),
        );
        let e = err(&ModSpec::Id("needy-mod".into()), &mods);
        assert!(e.contains("lib-mod"), "{e}");
        install(
            &mods,
            "lib-mod",
            &manifest("lib-mod", "allow_derived = true\n"),
        );
        install(
            &mods,
            "needy-two",
            &manifest(
                "needy-two",
                "requires = [\"lib-mod\"]\n[base]\nid = \"lib-mod\"\nversion = \"^1.0.0\"\n",
            ),
        );
        assert!(resolve_at(&ModSpec::Id("needy-two".into()), &mods, today()).is_ok());
    }

    #[test]
    fn difficulty_file_comes_from_the_last_layer_that_defines_it() {
        let mods = temp("difficulty-layers");
        let base = install(
            &mods,
            "base-mod",
            &manifest("base-mod", "allow_derived = true\n"),
        );
        let derived = install(
            &mods,
            "new-mod",
            &manifest("new-mod", "[base]\nid = 'base-mod'\nversion = '^1.0.0'\n"),
        );
        let stack = resolve_at(&ModSpec::Id("new-mod".into()), &mods, today()).unwrap();
        assert!(stack.difficulties_file().unwrap().is_none());
        fs::write(base.join(DIFFICULTIES_FILE), "base").unwrap();
        assert_eq!(stack.difficulties_file().unwrap().unwrap().text, "base");
        fs::write(derived.join(DIFFICULTIES_FILE), "derived").unwrap();
        assert_eq!(stack.difficulties_file().unwrap().unwrap().text, "derived");
    }

    #[test]
    fn acts_file_comes_from_the_last_layer_that_defines_it() {
        let mods = temp("acts-layers");
        let base = install(
            &mods,
            "base-mod",
            &manifest("base-mod", "allow_derived = true\n"),
        );
        let derived = install(
            &mods,
            "new-mod",
            &manifest("new-mod", "[base]\nid = 'base-mod'\nversion = '^1.0.0'\n"),
        );
        let stack = resolve_at(&ModSpec::Id("new-mod".into()), &mods, today()).unwrap();
        assert!(stack.acts_file().unwrap().is_none());
        fs::write(base.join(crate::layout::ACTS_FILE), "base").unwrap();
        assert_eq!(stack.acts_file().unwrap().unwrap().text, "base");
        fs::write(derived.join(crate::layout::ACTS_FILE), "derived").unwrap();
        assert_eq!(stack.acts_file().unwrap().unwrap().text, "derived");
    }

    #[test]
    fn lists_installed_mods_with_their_problems() {
        let mods = temp("list");
        install(&mods, "b-mod", &manifest("b-mod", ""));
        install(&mods, "a-mod", &manifest("a-mod", "").replace("1.0.0", "x"));
        fs::create_dir_all(mods.join(".hidden")).unwrap();
        fs::write(mods.join("stray-file.txt"), "x").unwrap();
        let list = list_installed(&mods);
        let names: Vec<&str> = list.iter().map(|i| i.folder.as_str()).collect();
        assert_eq!(names, ["a-mod", "b-mod"]);
        assert!(list[0].manifest.is_err());
        assert_eq!(list[1].manifest.as_ref().unwrap().id, "b-mod");
        assert!(list_installed(&mods.join("nowhere")).is_empty());
    }
}

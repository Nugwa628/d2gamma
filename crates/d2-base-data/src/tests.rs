//! The base folder's rules with stand-in steps: a "conversion" writes a
//! manifest that says `good`, and "loadable" accepts only that.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use super::*;

fn scratch(name: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("d2-base-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&p);
    fs::create_dir_all(&p).unwrap();
    p
}

/// A folder with the three server archives (empty files are enough for
/// the stand-in conversion).
fn diablo2(t: &Path) -> PathBuf {
    let d = t.join("Diablo II");
    fs::create_dir_all(&d).unwrap();
    for a in ["d2data.mpq", "D2exp.mpq", "Patch_D2.mpq", "d2char.mpq"] {
        fs::write(d.join(a), "").unwrap();
    }
    d
}

struct Fake {
    converts: AtomicUsize,
    check_fails: Mutex<bool>,
    said: Mutex<Vec<String>>,
    slow: Duration,
}

impl Fake {
    fn new() -> Fake {
        Fake {
            converts: AtomicUsize::new(0),
            check_fails: Mutex::new(false),
            said: Mutex::new(Vec::new()),
            slow: Duration::ZERO,
        }
    }

    fn convert(&self, _src: &Path, out: &Path) -> Result<(), String> {
        self.converts.fetch_add(1, Ordering::SeqCst);
        std::thread::sleep(self.slow);
        fs::create_dir_all(out.join("base/data")).unwrap();
        fs::create_dir_all(out.join(".work/tmp")).unwrap();
        fs::write(out.join("base/data/x.txt"), "x").unwrap();
        fs::write(out.join("report.txt"), "took 1 s").unwrap();
        fs::write(out.join("files.tsv"), "path\n").unwrap();
        fs::write(out.join("manifest.toml"), "good").unwrap();
        Ok(())
    }

    fn ensure(&self, home: &Path, key: &str, src: Option<&Path>) -> Result<Base, BaseError> {
        let convert = |s: &Path, o: &Path| self.convert(s, o);
        let check = |_: &Path| {
            if *self.check_fails.lock().unwrap() {
                Err("the probe game failed".to_owned())
            } else {
                Ok(())
            }
        };
        let say = |l: &str| self.said.lock().unwrap().push(l.to_owned());
        let steps = Steps {
            convert: &convert,
            loadable: &fake_loadable,
            check: &check,
            say: &say,
        };
        ensure_key(home, key, src, &steps)
    }

    fn converts(&self) -> usize {
        self.converts.load(Ordering::SeqCst)
    }
}

fn fake_loadable(dir: &Path) -> Result<(), String> {
    match fs::read_to_string(dir.join("manifest.toml")) {
        Ok(t) if t == "good" => Ok(()),
        Ok(t) => Err(format!(
            "kind `excel` is native_version 1, this build reads 2 ({t})"
        )),
        Err(e) => Err(e.to_string()),
    }
}

#[test]
fn the_key_names_the_format_and_changes_with_any_kind_version() {
    let kinds = |excel: u32| vec![("excel".to_owned(), excel), ("ds1".to_owned(), 1)];
    let k = key_of(1, &kinds(2), &SERVER_ARCHIVES);
    assert!(k.starts_with("f1-") && k.len() == 15, "{k}");
    assert!(
        !k.contains('.'),
        "a key is never taken for a .new or .old folder"
    );
    assert_eq!(k, key_of(1, &kinds(2), &SERVER_ARCHIVES), "stable");
    let mut reordered = kinds(2);
    reordered.reverse();
    assert_eq!(
        k,
        key_of(1, &reordered, &SERVER_ARCHIVES),
        "kind order is not part of it"
    );
    assert_ne!(
        k,
        key_of(1, &kinds(1), &SERVER_ARCHIVES),
        "the excel bump (old roots refused)"
    );
    assert_ne!(k, key_of(2, &kinds(2), &SERVER_ARCHIVES), "a format bump");
    assert_ne!(k, key_of(1, &kinds(2), &["d2data.mpq"]), "other archives");
    assert!(format_key().starts_with(&format!("f{}-", d2_native::manifest::FORMAT_VERSION)));
}

#[test]
fn the_first_start_converts_and_the_next_reuses_it() {
    let t = scratch("first");
    let home = t.join("base");
    let src = diablo2(&t);
    let f = Fake::new();
    let b = f.ensure(&home, "f1-aaaa", Some(&src)).unwrap();
    assert_eq!(f.converts(), 1);
    assert!(b.converted.is_some());
    assert_eq!(b.dir, home.join("f1-aaaa"));
    assert!(home.join("f1-aaaa.ok").is_file());
    // Only what every machine makes the same stays inside.
    assert!(!b.dir.join("report.txt").exists() && !b.dir.join(".work").exists());
    assert!(home.join("f1-aaaa.report.txt").is_file());
    assert!(!home.join("f1-aaaa.new").exists());
    drop(b);
    let b = f.ensure(&home, "f1-aaaa", Some(&src)).unwrap();
    assert_eq!(f.converts(), 1, "converted once");
    assert!(b.converted.is_none());
    assert_eq!(
        state(&home),
        State::Stale(vec!["f1-aaaa".into()]),
        "another key than this build's is an old format"
    );
    let _ = fs::remove_dir_all(&t);
}

#[test]
fn an_old_or_refused_marker_converts_again_by_itself() {
    let t = scratch("refused");
    let home = t.join("base");
    let src = diablo2(&t);
    let f = Fake::new();
    drop(f.ensure(&home, "f1-aaaa", Some(&src)).unwrap());
    // The folder's manifest is from before a kind bump: this build refuses it.
    fs::write(home.join("f1-aaaa/manifest.toml"), "excel = 1").unwrap();
    let b = f.ensure(&home, "f1-aaaa", Some(&src)).unwrap();
    assert_eq!(f.converts(), 2);
    assert_eq!(
        fs::read_to_string(b.dir.join("manifest.toml")).unwrap(),
        "good"
    );
    assert!(!home.join("f1-aaaa.old").exists() && !home.join("f1-aaaa.new").exists());
    let said = f.said.lock().unwrap().join("\n");
    assert!(
        said.contains("is refused (kind `excel` is native_version 1"),
        "{said}"
    );
    // A folder a killed conversion left (no `.ok`) is made again too.
    drop(b);
    fs::remove_file(home.join("f1-aaaa.ok")).unwrap();
    drop(f.ensure(&home, "f1-aaaa", Some(&src)).unwrap());
    assert_eq!(f.converts(), 3);
    let _ = fs::remove_dir_all(&t);
}

#[test]
fn the_old_format_stays_until_the_new_one_passed_its_check() {
    let t = scratch("swap");
    let home = t.join("base");
    let src = diablo2(&t);
    let f = Fake::new();
    drop(f.ensure(&home, "f1-old", Some(&src)).unwrap());
    *f.check_fails.lock().unwrap() = true;
    let e = f.ensure(&home, "f1-new", Some(&src)).unwrap_err();
    assert!(matches!(e, BaseError::Check(_)), "{e}");
    assert!(e.to_string().contains("the probe game failed"), "{e}");
    assert!(home.join("f1-old").is_dir(), "the old format is kept");
    assert!(!home.join("f1-new").exists() && !home.join("f1-new.new").exists());
    *f.check_fails.lock().unwrap() = false;
    // A server of the old format still runs: its folder stays.
    let old = f.ensure(&home, "f1-old", Some(&src)).unwrap();
    let new = f.ensure(&home, "f1-new", Some(&src)).unwrap();
    assert!(home.join("f1-old").is_dir(), "held by a running server");
    drop(old);
    drop(new);
    // The next start finds it unused and removes it.
    drop(f.ensure(&home, "f1-new", Some(&src)).unwrap());
    assert!(!home.join("f1-old").exists() && !home.join("f1-old.ok").exists());
    assert!(home.join("f1-new").is_dir());
    let _ = fs::remove_dir_all(&t);
}

#[test]
fn two_servers_starting_at_once_convert_once() {
    let t = scratch("lock");
    let home = t.join("base");
    let src = diablo2(&t);
    let mut f = Fake::new();
    f.slow = Duration::from_millis(400);
    let f = &f;
    let (a, b) = std::thread::scope(|s| {
        let a = s.spawn(|| {
            f.ensure(&home, "f1-aaaa", Some(&src))
                .map(|b| b.converted.is_some())
        });
        let b = s.spawn(|| {
            f.ensure(&home, "f1-aaaa", Some(&src))
                .map(|b| b.converted.is_some())
        });
        (a.join().unwrap().unwrap(), b.join().unwrap().unwrap())
    });
    assert_eq!(f.converts(), 1, "the second waited and found it made");
    assert!(a ^ b, "one of them converted");
    let said = f.said.lock().unwrap().join("\n");
    assert!(
        said.contains("another server is converting it; waiting"),
        "{said}"
    );
    let _ = fs::remove_dir_all(&t);
}

#[test]
fn no_diablo2_names_the_one_step_that_fixes_it() {
    let t = scratch("none");
    let home = t.join("base");
    let f = Fake::new();
    let e = f.ensure(&home, "f1-aaaa", None).unwrap_err();
    assert!(matches!(e, BaseError::NoDiablo2 { .. }));
    let text = e.to_string();
    assert!(text.contains("server-ctl import-d2"), "{text}");
    assert!(text.contains("node-ctl.ps1 import-d2"), "{text}");
    let partial = t.join("partial");
    fs::create_dir_all(&partial).unwrap();
    fs::write(partial.join("D2DATA.MPQ"), "").unwrap();
    let e = f
        .ensure(&home, "f1-aaaa", Some(&partial))
        .unwrap_err()
        .to_string();
    assert!(e.contains("has no d2exp.mpq, patch_d2.mpq"), "{e}");
    let e = f
        .ensure(&home, "f1-aaaa", Some(&t.join("nope")))
        .unwrap_err()
        .to_string();
    assert!(e.contains("no Diablo II folder at"), "{e}");
    assert_eq!(f.converts(), 0);
    let _ = fs::remove_dir_all(&t);
}

#[test]
fn detection_takes_the_first_folder_with_the_server_archives() {
    let t = scratch("detect");
    let full = diablo2(&t);
    let partial = t.join("partial");
    fs::create_dir_all(&partial).unwrap();
    fs::write(partial.join("d2data.mpq"), "").unwrap();
    let c = vec![t.join("missing"), partial, full.clone()];
    assert_eq!(detect_in(&c), Some(full));
    assert_eq!(detect_in(&c[..2]), None);
    let reg = "\r\nHKEY_CURRENT_USER\\Software\\Blizzard Entertainment\\Diablo II\r\n    InstallPath    REG_SZ    C:\\Games\\Diablo II\r\n";
    assert_eq!(
        parse_reg_value(reg),
        Some(PathBuf::from("C:\\Games\\Diablo II"))
    );
    assert_eq!(parse_reg_value("ERROR: not found"), None);
    let _ = fs::remove_dir_all(&t);
}

fn steps_with<'a>(convert: &'a dyn Fn(&Path, &Path) -> Result<(), String>) -> Steps<'a> {
    Steps {
        convert,
        loadable: &fake_loadable,
        check: &|_| Ok(()),
        say: &|_| {},
    }
}

#[test]
fn an_install_or_an_own_converted_folder_is_read_as_before() {
    let t = scratch("legacy");
    let f = Fake::new();
    let convert = |s: &Path, o: &Path| f.convert(s, o);
    let steps = steps_with(&convert);
    let install = diablo2(&t);
    let req = Request {
        game_data: &install,
        diablo2: None,
        base_home: Some(&t.join("base")),
        view: &t.join("view"),
    };
    let r = resolve(&req, &steps).unwrap();
    assert_eq!(r.root, install);
    assert!(r.base.is_none() && r.hint.is_none());
    // Any folder with the base archive is an install, as before (a test
    // setup's made-up game folder).
    let made_up = t.join("made-up");
    fs::create_dir_all(&made_up).unwrap();
    fs::write(made_up.join("d2data.mpq"), "archive").unwrap();
    let r = resolve(
        &Request {
            game_data: &made_up,
            ..req.clone()
        },
        &steps,
    )
    .unwrap();
    assert_eq!(r.root, made_up);
    // Today's `game/`: a converted copy, used with the one-line hint.
    let own = t.join("game");
    fs::create_dir_all(own.join("base")).unwrap();
    fs::write(own.join("manifest.toml"), "good").unwrap();
    let req = Request {
        game_data: &own,
        ..req
    };
    let r = resolve(&req, &steps).unwrap();
    assert_eq!(r.root, own);
    assert!(r.hint.as_deref().unwrap().contains("server-ctl import-d2"));
    assert_eq!(f.converts(), 0);
    // Once Diablo II is configured, the shared base takes over.
    let req = Request {
        diablo2: Some(&install),
        ..req
    };
    let r = resolve(&req, &steps).unwrap();
    assert_eq!(r.root, t.join("base").join(format_key()));
    assert_eq!(f.converts(), 1);
    let _ = fs::remove_dir_all(&t);
}

#[test]
fn a_mod_package_reads_its_layer_over_the_shared_base() {
    let t = scratch("mod");
    let f = Fake::new();
    let convert = |s: &Path, o: &Path| f.convert(s, o);
    let steps = steps_with(&convert);
    let install = diablo2(&t);
    let pkg = t.join("pkg/game");
    fs::create_dir_all(pkg.join("mods/mymod")).unwrap();
    fs::write(pkg.join("mods/mymod/mod.toml"), "name = \"mymod\"").unwrap();
    let base_home = t.join("base");
    let req = |view: &Path| {
        let r = Request {
            game_data: &pkg,
            diablo2: Some(&install),
            base_home: Some(&base_home),
            view,
        };
        resolve(&r, &steps).unwrap()
    };
    // A realm and a game server of the mod, one base.
    let realm = req(&t.join("realm/game-view"));
    let node = req(&t.join("node/game-view"));
    assert_eq!(f.converts(), 1);
    for r in [&realm, &node] {
        assert!(r.root.join("mods/mymod/mod.toml").is_file());
        assert_eq!(
            fs::read_to_string(r.root.join("base/data/x.txt")).unwrap(),
            "x"
        );
        assert_eq!(
            fs::read_to_string(r.root.join("manifest.toml")).unwrap(),
            "good"
        );
    }
    assert_ne!(realm.root, node.root, "each server has its own view");
    // And the base game's own servers read the base itself.
    let none = t.join("no-game");
    let plain = resolve(
        &Request {
            game_data: &none,
            diablo2: Some(&install),
            base_home: Some(&base_home),
            view: &t.join("d2/game-view"),
        },
        &steps,
    )
    .unwrap();
    assert_eq!(plain.root, base_home.join(format_key()));
    assert_eq!(f.converts(), 1);
    let _ = fs::remove_dir_all(&t);
}

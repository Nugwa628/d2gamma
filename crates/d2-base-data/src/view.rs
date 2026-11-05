//! A mod server's view of its layer over the shared base.
//!
//! The native reader opens one root: its `manifest.toml`, `files.tsv`,
//! `base/` and `mods/`. A mod server's package ships `mods/` only, so the
//! server reads a view folder of its own, `<view>/<key>/`: copies of the
//! base's two small files, `base` a link to the base's `base/`, and in
//! `mods/` a link to each entry of the package's `mods/` (a layer folder or
//! a pack file). Each format has its own view, so a server still running
//! the older format during an upgrade keeps reading its own.
//!
//! Links are symbolic links on Linux. On Windows a folder is a junction
//! (no administrator needed) and a file a hard link, or a copy across
//! drives.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Makes (or brings up to date) the view `<view>/<key>/` of `mods` over
/// the base folder `base`, and returns it.
pub fn make(view: &Path, key: &str, base: &Path, mods: &Path) -> io::Result<PathBuf> {
    let root = view.join(key);
    fs::create_dir_all(root.join("mods"))?;
    for f in ["manifest.toml", "files.tsv"] {
        copy_if_changed(&base.join(f), &root.join(f))?;
    }
    link_dir(&base.join("base"), &root.join("base"))?;
    let mut names = Vec::new();
    for e in fs::read_dir(mods)? {
        let e = e?;
        let name = e.file_name();
        let from = e.path();
        let to = root.join("mods").join(&name);
        if from.is_dir() {
            link_dir(&from, &to)?;
        } else {
            link_file(&from, &to)?;
        }
        names.push(name);
    }
    for e in fs::read_dir(root.join("mods"))? {
        let e = e?;
        if !names.contains(&e.file_name()) {
            remove_link(&e.path())?;
        }
    }
    Ok(root)
}

/// Removes the views of formats whose base folder is gone (`live`: the
/// formats still there); a server of an older format keeps its view
/// while its base is there.
pub fn drop_stale(view: &Path, live: &[String]) {
    let Ok(r) = fs::read_dir(view) else { return };
    for e in r.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if e.path().is_dir() && !live.contains(&name) {
            remove_view(&e.path());
        }
    }
}

/// Removes a view: its links, never what they point at.
fn remove_view(root: &Path) {
    if let Ok(r) = fs::read_dir(root.join("mods")) {
        for e in r.flatten() {
            let _ = remove_link(&e.path());
        }
    }
    let _ = remove_link(&root.join("base"));
    let _ = fs::remove_dir(root.join("mods"));
    for f in ["manifest.toml", "files.tsv"] {
        let _ = fs::remove_file(root.join(f));
    }
    let _ = fs::remove_dir(root);
}

fn copy_if_changed(from: &Path, to: &Path) -> io::Result<()> {
    let new = fs::read(from)?;
    if fs::read(to).ok().as_deref() == Some(&new[..]) {
        return Ok(());
    }
    let tmp = to.with_extension("tmp");
    fs::write(&tmp, &new)?;
    fs::rename(&tmp, to)
}

/// Whether `link` already is a link to `target`.
fn points_at(link: &Path, target: &Path) -> bool {
    fs::read_link(link).is_ok_and(|t| t == target)
}

/// Removes a link (or a copied file) at `p`, never a real folder's files.
fn remove_link(p: &Path) -> io::Result<()> {
    let meta = match fs::symlink_metadata(p) {
        Ok(m) => m,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e),
    };
    if meta.is_dir() {
        // A Windows junction reads as a folder; removing it removes the
        // link alone. A real folder here is not ours: leave its files.
        fs::remove_dir(p)
    } else {
        fs::remove_file(p).or_else(|e| {
            // A link to a folder on Windows is removed as a folder.
            if cfg!(windows) {
                fs::remove_dir(p)
            } else {
                Err(e)
            }
        })
    }
}

fn link_dir(target: &Path, link: &Path) -> io::Result<()> {
    if points_at(link, target) {
        return Ok(());
    }
    remove_link(link)?;
    make_dir_link(target, link)
}

fn link_file(target: &Path, link: &Path) -> io::Result<()> {
    if points_at(link, target) {
        return Ok(());
    }
    remove_link(link)?;
    make_file_link(target, link)
}

#[cfg(unix)]
fn make_dir_link(target: &Path, link: &Path) -> io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}

#[cfg(unix)]
fn make_file_link(target: &Path, link: &Path) -> io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}

#[cfg(windows)]
fn make_dir_link(target: &Path, link: &Path) -> io::Result<()> {
    let target = std::path::absolute(target)?;
    let out = std::process::Command::new("cmd")
        .arg("/C")
        .arg("mklink")
        .arg("/J")
        .arg(link)
        .arg(&target)
        .output()?;
    if out.status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "mklink /J {} {}: {}",
            link.display(),
            target.display(),
            String::from_utf8_lossy(&out.stderr).trim()
        )))
    }
}

#[cfg(windows)]
fn make_file_link(target: &Path, link: &Path) -> io::Result<()> {
    fs::hard_link(target, link).or_else(|_| fs::copy(target, link).map(|_| ()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("d2-base-view-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn a_view_puts_the_layer_over_the_base_and_follows_it() {
        let t = scratch("over");
        let base = t.join("base/f1-k");
        fs::create_dir_all(base.join("base/data")).unwrap();
        fs::write(base.join("base/data/a.txt"), "a").unwrap();
        fs::write(base.join("manifest.toml"), "m1").unwrap();
        fs::write(base.join("files.tsv"), "t1").unwrap();
        let mods = t.join("pkg/mods");
        fs::create_dir_all(mods.join("mymod/files")).unwrap();
        fs::write(mods.join("mymod/mod.toml"), "name").unwrap();
        fs::write(mods.join("other.d2pack"), "pack").unwrap();
        let view = t.join("view");
        let root = make(&view, "f1-k", &base, &mods).unwrap();
        assert_eq!(root, view.join("f1-k"));
        assert_eq!(
            fs::read_to_string(root.join("base/data/a.txt")).unwrap(),
            "a"
        );
        assert_eq!(
            fs::read_to_string(root.join("manifest.toml")).unwrap(),
            "m1"
        );
        assert_eq!(
            fs::read_to_string(root.join("mods/mymod/mod.toml")).unwrap(),
            "name"
        );
        assert_eq!(
            fs::read_to_string(root.join("mods/other.d2pack")).unwrap(),
            "pack"
        );
        // Again, after the base and the package changed.
        fs::write(base.join("manifest.toml"), "m2").unwrap();
        fs::remove_file(mods.join("other.d2pack")).unwrap();
        make(&view, "f1-k", &base, &mods).unwrap();
        assert_eq!(
            fs::read_to_string(root.join("manifest.toml")).unwrap(),
            "m2"
        );
        assert!(!root.join("mods/other.d2pack").exists());
        // Another format: its own view; the old one goes, its targets stay.
        let base2 = t.join("base/f2-k");
        fs::create_dir_all(base2.join("base")).unwrap();
        fs::write(base2.join("manifest.toml"), "n").unwrap();
        fs::write(base2.join("files.tsv"), "n").unwrap();
        let root2 = make(&view, "f2-k", &base2, &mods).unwrap();
        assert!(root2.join("mods/mymod/mod.toml").is_file());
        drop_stale(&view, &["f1-k".into(), "f2-k".into()]);
        assert!(
            root.join("mods/mymod").exists(),
            "kept while its base is there"
        );
        drop_stale(&view, &["f2-k".into()]);
        assert!(!root.exists(), "the old format's view is removed");
        assert!(base.join("base/data/a.txt").is_file());
        assert!(mods.join("mymod/mod.toml").is_file());
        let _ = fs::remove_dir_all(&t);
    }
}

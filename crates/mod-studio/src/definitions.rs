//! Conflict-checked optional gameplay documents used by the definition forms.
use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use toml::Table;

pub struct Document {
    path: PathBuf,
    pub table: Table,
    saved: Option<String>,
    initial: Table,
    pub error: Option<String>,
    validate: fn(&str) -> Result<()>,
}
impl Document {
    pub fn open(
        root: &Path,
        relative: &str,
        initial: &str,
        validate: fn(&str) -> Result<()>,
    ) -> Self {
        let path = root.join(relative);
        let initial: Table = initial.parse().expect("built-in definition");
        let loaded = std::fs::read_to_string(&path);
        let mut document = Self {
            path,
            table: initial.clone(),
            initial,
            saved: None,
            error: None,
            validate,
        };
        match loaded {
            Ok(text) => match validate(&text).and_then(|()| text.parse().map_err(Into::into)) {
                Ok(table) => {
                    document.table = table;
                    document.saved = Some(text);
                }
                Err(error) => document.error = Some(format!("{error:#}")),
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => document.error = Some(error.to_string()),
        }
        document
    }
    pub fn dirty(&self) -> bool {
        match &self.saved {
            Some(saved) => saved.parse::<Table>().ok().as_ref() != Some(&self.table),
            None => self.table != self.initial,
        }
    }
    pub fn validate(&self) -> Result<()> {
        if let Some(error) = &self.error {
            bail!("{}: {error}", self.path.display());
        }
        if !self.dirty() {
            return Ok(());
        }
        (self.validate)(&toml::to_string(&self.table)?)?;
        let parent = self.path.parent().context("definition parent")?;
        for path in [parent, self.path.as_path()] {
            if std::fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink()) {
                bail!("Definition path must stay inside the project");
            }
        }
        let disk = match std::fs::read_to_string(&self.path) {
            Ok(text) => Some(text),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(error.into()),
        };
        if disk != self.saved {
            bail!(
                "{} changed outside Studio; reopen the project",
                self.path.display()
            );
        }
        Ok(())
    }
    pub fn save(&mut self) -> Result<()> {
        self.validate()?;
        if !self.dirty() {
            return Ok(());
        }
        let text = toml::to_string_pretty(&self.table)?;
        std::fs::create_dir_all(self.path.parent().context("definition parent")?)?;
        std::fs::write(&self.path, &text)?;
        self.saved = Some(text);
        Ok(())
    }
}

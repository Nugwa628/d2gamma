//! Schema-driven working tables over an immutable install and existing patches.
//! Only the Studio overlay diff is written; existing author patches stay intact.
use anyhow::{bail, Context, Result};
use d2_data::{
    bin::DataRules,
    patch::{self, Origin, PatchData, PatchTable, Row, Writer},
};
use serde::Deserialize;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

pub const OVERLAY: &str = "~studio.d2patch";

#[derive(Clone, Deserialize)]
pub struct Column {
    pub name: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub references: Vec<String>,
    pub reference_encoding: String,
    pub target_column: Option<String>,
    pub zero_is_none: bool,
}
#[derive(Deserialize)]
pub struct TableSchema {
    pub columns: Vec<Column>,
    pub max_rows_mod: Option<usize>,
}
pub fn schemas() -> BTreeMap<String, TableSchema> {
    d2_data::schema::export_tables()
        .into_iter()
        .map(|(k, v)| {
            (
                k,
                serde_json::from_str(&v).expect("generated editor schema"),
            )
        })
        .collect()
}

pub struct Tables {
    pub baseline: PatchData,
    reference_base: PatchData,
    pub edited: PatchData,
    pub schemas: BTreeMap<String, TableSchema>,
    pub native: Option<PathBuf>,
    pub drop_rules: d2_sim::treasure::rules::DropRules,
    pub revision: u64,
    pub dirty: bool,
    pub problems: Vec<String>,
    path: PathBuf,
    layers: Option<d2_native::layers::LayerStack>,
    snapshot: Vec<(String, Vec<u8>)>,
}
impl Tables {
    pub fn load(native: &Path, installed: &Path, project: &Path) -> Result<Self> {
        let resolved =
            d2_modkit::resolve(&d2_modkit::ModSpec::Dir(project.canonicalize()?), installed)?;
        if resolved.picked().manifest.screen_only {
            bail!("Screen-only add-ons cannot edit game tables");
        }
        let layers = d2_native::layers::LayerStack::with_mods(native, &resolved.layer_entries())?;
        let rules = patch::rules_for(DataRules::Mod);
        let baseline = PatchData::from_base(&rules, &mut |file| {
            layers
                .read_in(
                    &layers.layers[0],
                    &format!("data/global/excel/{}", file.to_ascii_lowercase()),
                )
                .map(|r| {
                    r.map(|bytes| ("native".into(), bytes))
                        .map_err(|e| e.to_string())
                })
                .transpose()
        })
        .map_err(|f| anyhow::anyhow!(report(&f)))?;
        let reference_base = baseline.clone();
        let snapshot = layers.patches()?;
        let last_prefix = format!(
            "mods/{}/files/patches/",
            layers.layers.last().context("mod layer")?.name
        );
        let overlay_path = format!("{last_prefix}{OVERLAY}");
        let mut baseline = baseline;
        let mut overlay = None;
        for (path, bytes) in &snapshot {
            if path == &overlay_path {
                overlay = Some(bytes.clone());
                continue;
            }
            if path.starts_with(&last_prefix) && path > &overlay_path {
                bail!("Patch {path} sorts after the Studio overlay; rename it before editing");
            }
            apply(&mut baseline, path, bytes)?;
        }
        let mut editor = Self::from_data(baseline, project.join("files/patches").join(OVERLAY));
        if let Some(bytes) = overlay {
            apply(&mut editor.edited, OVERLAY, &bytes)?;
        }
        editor.drop_rules = resolved
            .rules_file()?
            .map(|f| d2_sim::rules::global::GlobalRules::from_toml(&f.text))
            .transpose()?
            .unwrap_or_default()
            .drops;
        editor.native = Some(native.to_owned());
        editor.reference_base = reference_base;
        editor.layers = Some(layers);
        editor.snapshot = snapshot;
        editor.validate();
        Ok(editor)
    }
    pub fn from_data(baseline: PatchData, path: PathBuf) -> Self {
        Self {
            reference_base: baseline.clone(),
            edited: baseline.clone(),
            baseline,
            schemas: schemas(),
            native: None,
            drop_rules: d2_sim::treasure::rules::DropRules::defaults(),
            revision: 0,
            dirty: false,
            problems: Vec::new(),
            path,
            layers: None,
            snapshot: Vec::new(),
        }
    }
    /// The tables as they were when this session started: what a copied base
    /// skill is compared with.
    pub fn reference(&self) -> &PatchData {
        &self.reference_base
    }
    /// Makes the current baseline the reference. For tests that build their
    /// own tables.
    #[doc(hidden)]
    pub fn use_baseline_as_reference(&mut self) {
        self.reference_base = self.baseline.clone();
    }
    pub fn column(&self, table: usize, column: usize) -> Option<&Column> {
        let t = &self.edited.tables[table];
        self.schemas
            .get(t.name())?
            .columns
            .iter()
            .find(|c| c.name.as_bytes().eq_ignore_ascii_case(&t.header[column]))
    }
    pub fn changed(&self, t: usize, r: usize, c: usize) -> bool {
        self.reference_base.tables[t]
            .rows
            .get(r)
            .and_then(|row| row.cells.get(c))
            != Some(&self.edited.tables[t].rows[r].cells[c])
    }
    pub fn diff(&self) -> Result<Vec<u8>> {
        let mut additions = String::new();
        let rendered: Vec<_> = self
            .edited
            .tables
            .iter()
            .zip(&self.baseline.tables)
            .map(|(table, base)| {
                let mut table = table.clone();
                if let Some(c) = table
                    .header
                    .iter()
                    .position(|h| h == patch::DISABLED_COLUMN)
                {
                    for r in base.rows.len()..table.rows.len() {
                        if table.disabled(r) {
                            additions.push_str(&format!(
                                "\ntable {}\nremove #{r} {}\n",
                                table.name(),
                                text(&patch::canonical_token(table.key(r)))
                            ));
                            table.rows[r].cells[c].clear();
                        }
                    }
                }
                table.render()
            })
            .collect();
        let edits: Vec<_> = self
            .baseline
            .tables
            .iter()
            .zip(&rendered)
            .map(|(t, b)| (t, b.as_slice()))
            .collect();
        let mut bytes =
            patch::diff_tables(&edits, true).map_err(|e| anyhow::anyhow!(e.to_string()))?;
        bytes.extend_from_slice(additions.as_bytes());
        for edit in self.edited.strings.iter().skip(self.baseline.strings.len()) {
            let (kind, key, text) = match &edit.edit {
                d2_data::strings::StringEdit::Add { key, text } => ("stradd", key, text),
                d2_data::strings::StringEdit::Set { key, text } => ("strset", key, text),
            };
            bytes.extend_from_slice(
                format!(
                    "\n{kind} {} {}\n",
                    String::from_utf8(patch::canonical_token(key))?,
                    String::from_utf8(patch::canonical_token(text))?
                )
                .as_bytes(),
            );
        }

        Ok(bytes)
    }

    /// Add a translated UI label or explicitly replace an existing string.
    pub fn edit_string(&mut self, key: &str, text: &str, replace: bool) -> Result<()> {
        anyhow::ensure!(
            !key.is_empty() && key.len() <= 512 && !key.contains('\0') && !text.contains('\0'),
            "Use a nonempty string key up to 512 bytes, without NUL bytes"
        );
        let edit = if replace {
            d2_data::strings::StringEdit::Set {
                key: key.as_bytes().to_vec(),
                text: text.as_bytes().to_vec(),
            }
        } else {
            d2_data::strings::StringEdit::Add {
                key: key.as_bytes().to_vec(),
                text: text.as_bytes().to_vec(),
            }
        };
        let start = self.baseline.strings.len();
        if let Some(old) = self.edited.strings[start..]
            .iter_mut()
            .find(|s| match &s.edit {
                d2_data::strings::StringEdit::Add { key: k, .. }
                | d2_data::strings::StringEdit::Set { key: k, .. } => k == key.as_bytes(),
            })
        {
            old.edit = edit;
        } else {
            self.edited.strings.push(patch::StrEdit {
                edit,
                path: OVERLAY.into(),
                pos: 1,
                line: 1,
                col: 1,
            });
        }
        self.dirty = true;
        self.validate();
        Ok(())
    }
    pub fn validate(&mut self) {
        self.revision = self.revision.wrapping_add(1);
        let mut problems = Vec::new();
        for (t, table) in self.edited.tables.iter().enumerate() {
            for (r, row) in table.rows.iter().enumerate() {
                if table.disabled(r) {
                    continue;
                }
                for (c, cell) in row.cells.iter().enumerate() {
                    if !self.changed(t, r, c) {
                        continue;
                    }
                    if let Err(e) = validate_cell(self.column(t, c), cell) {
                        problems.push(format!(
                            "{} · row {r} · {}: {e}",
                            table.name(),
                            text(&table.header[c])
                        ));
                    }
                }
            }
        }
        match self.diff() {
            Ok(bytes) => {
                let mut data = self.baseline.clone();
                if let Err(e) = apply(&mut data, OVERLAY, &bytes) {
                    problems.push(e.to_string());
                }
                problems.extend(
                    patch::check_references(&self.reference_base, &data)
                        .iter()
                        .map(ToString::to_string),
                );
            }
            Err(e) => problems.push(e.to_string()),
        }
        self.problems = problems;
    }
    pub fn save(&mut self) -> Result<()> {
        self.validate();
        if !self.problems.is_empty() {
            bail!(
                "Fix the table problems before saving:\n{}",
                self.problems.join("\n")
            );
        }
        if let Some(layers) = &self.layers {
            if layers.patches()? != self.snapshot {
                bail!("Patch files changed outside Studio. Reopen the project before saving.");
            }
        }
        let bytes = self.diff()?;
        fs::create_dir_all(self.path.parent().context("patch folder")?)?;
        fs::write(&self.path, bytes)?;
        if let Some(layers) = &self.layers {
            self.snapshot = layers.patches()?;
        }
        self.dirty = false;
        Ok(())
    }
    pub fn add(&mut self, t: usize, duplicate: Option<usize>, key: &str) -> Result<usize> {
        let table = &mut self.edited.tables[t];
        if table.rules.fixed {
            bail!("This table has a fixed row count");
        }
        if self
            .schemas
            .get(table.name())
            .and_then(|s| s.max_rows_mod)
            .is_some_and(|n| table.rows.len() >= n)
        {
            bail!("Table row limit reached");
        }
        if key.trim().is_empty()
            || table.rows.iter().any(|r| {
                table.rules.kind.normalize(&r.cells[table.key_col])
                    == table.rules.kind.normalize(key.as_bytes())
            })
        {
            bail!("Choose a new, nonempty row key");
        }
        let mut cells = duplicate
            .map(|r| table.rows[r].cells.clone())
            .unwrap_or_else(|| vec![Vec::new(); table.header.len()]);
        cells[table.key_col] = key.as_bytes().to_vec();
        if let Some(c) = table
            .header
            .iter()
            .position(|h| h == patch::DISABLED_COLUMN)
        {
            cells[c].clear();
        }
        let r = table.rows.len();
        table.rows.push(Row {
            writers: vec![Writer::Base; cells.len()],
            cells,
            origin: Origin::Base(r + 2),
        });
        self.dirty = true;
        self.validate();
        Ok(r)
    }
    pub fn disable(&mut self, t: usize, r: usize) -> Result<()> {
        // Diff's disable statement retains the original cells and their reserved id.
        let base = &self.baseline.tables[t];
        let table = &mut self.edited.tables[t];
        let c = match table
            .header
            .iter()
            .position(|h| h == patch::DISABLED_COLUMN)
        {
            Some(c) => c,
            None => {
                table.header.push(patch::DISABLED_COLUMN.to_vec());
                for row in &mut table.rows {
                    row.cells.push(Vec::new());
                    row.writers.push(Writer::Base);
                }
                table.header.len() - 1
            }
        };
        let mut cells = base.rows.get(r).unwrap_or(&table.rows[r]).cells.clone();
        cells.resize(table.header.len(), Vec::new());
        cells[c] = b"1".to_vec();
        table.rows[r].cells = cells;
        self.dirty = true;
        self.validate();
        Ok(())
    }
    pub fn restore(&mut self, t: usize, r: usize) {
        if let Some(base) = self.baseline.tables[t].rows.get(r) {
            let width = self.edited.tables[t].header.len();
            let mut row = base.clone();
            row.cells.resize(width, Vec::new());
            row.writers.resize(width, Writer::Base);
            self.edited.tables[t].rows[r] = row;
            self.dirty = true;
            self.validate();
        }
    }
    pub fn choices(&self, column: &Column) -> Vec<(String, String)> {
        let mut out = Vec::new();
        for target in &column.references {
            let Some(table) = self.edited.table(target.trim_end_matches(".txt")) else {
                continue;
            };
            for (r, row) in table.rows.iter().enumerate() {
                if table.disabled(r) {
                    continue;
                }
                let value = if column.reference_encoding == "id" {
                    if let Some(c) = column.target_column.as_ref().and_then(|c| {
                        table
                            .header
                            .iter()
                            .position(|h| h.eq_ignore_ascii_case(c.as_bytes()))
                    }) {
                        text(&row.cells[c])
                    } else {
                        r.to_string()
                    }
                } else {
                    text(table.key(r))
                };
                let name = row_name(table, r);
                out.push((
                    value.clone(),
                    format!("{name}  ·  {value}  ({})", table.name()),
                ));
            }
        }
        if column.zero_is_none {
            out.insert(0, ("0".into(), "None".into()));
        }
        out
    }
    pub fn search(&self, query: &str) -> Vec<(usize, usize, String)> {
        let q = query.to_lowercase();
        self.edited
            .tables
            .iter()
            .enumerate()
            .flat_map(|(t, table)| {
                let q = &q;
                table
                    .rows
                    .iter()
                    .enumerate()
                    .filter(move |(_, row)| {
                        row.cells.iter().any(|v| text(v).to_lowercase().contains(q))
                    })
                    .map(move |(r, _)| (t, r, format!("{} · {}", table.name(), row_name(table, r))))
            })
            .collect()
    }
}
pub fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}
pub fn row_name(table: &PatchTable, r: usize) -> String {
    for name in ["name", "ItemType", "NameStr", "index", "skill", "Id"] {
        if let Some(c) = table
            .header
            .iter()
            .position(|h| h.eq_ignore_ascii_case(name.as_bytes()))
        {
            if !table.rows[r].cells[c].is_empty() {
                return text(&table.rows[r].cells[c]);
            }
        }
    }
    text(table.key(r))
}
fn report(findings: &[patch::Finding]) -> String {
    findings
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n")
}
fn apply(data: &mut PatchData, path: &str, bytes: &[u8]) -> Result<()> {
    let (layer, mut findings) = patch::parse_layer(path, bytes, 0);
    if !patch::has_errors(&findings) {
        findings.extend(patch::apply_stack(data, &[layer], "Studio"));
    }
    if patch::has_errors(&findings) {
        bail!("{}", report(&findings));
    }
    Ok(())
}
pub fn validate_cell(column: Option<&Column>, value: &[u8]) -> Result<()> {
    if value.iter().any(|b| matches!(b, 0 | b'\t' | b'\n' | b'\r')) {
        bail!("Cells cannot contain tabs, line breaks or NUL");
    }
    let Some(column) = column else {
        return Ok(());
    };
    if value.is_empty() {
        return Ok(());
    }
    let range = match column.kind.as_str() {
        "u8" => Some((-128, 255)),
        "u16" => Some((-32768, 65535)),
        "u32" => Some((i32::MIN as i64, u32::MAX as i64)),
        "bit" => Some((0, 1)),
        _ => None,
    };
    if let Some((min, max)) = range {
        let n: i64 = std::str::from_utf8(value)?
            .parse()
            .context("Enter a whole number")?;
        if n < min || n > max {
            bail!("Enter a number from {min} to {max}");
        }
    }
    Ok(())
}

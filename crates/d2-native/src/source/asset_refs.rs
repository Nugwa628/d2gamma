//! File and string references introduced by table patches. Unchanged base
//! cells retain the install's optional references; new cells must resolve
//! against the complete stack, including later layers and string edits.

use d2_data::patch::{self, PatchData};
use d2_data::schema::{schema, FieldType};
use d2_data::strings::StringTables;
use d2_data::txt::bind;

use super::{fold, NativeSource};

pub(super) fn check(
    src: &NativeSource,
    base: &PatchData,
    data: &PatchData,
    strings: &StringTables,
) -> Result<(), String> {
    let mut strings = strings.clone();
    let findings = patch::apply_strings(&mut strings, &data.strings);
    if patch::has_errors(&findings) {
        return Err(super::tables::findings_text(&findings));
    }
    let mut errors = Vec::new();
    for table in &data.tables {
        let before = base.table(table.name());
        let str_cols: Vec<usize> = schema()
            .called()
            .filter(|d| d.txt_name == table.rules.txt_name)
            .flat_map(|d| {
                let names: Vec<_> = d.fields.iter().map(|f| f.column.as_slice()).collect();
                let binding = bind(&table.header, &names);
                d.fields
                    .iter()
                    .enumerate()
                    .filter_map(|(i, f)| {
                        (f.field_type == FieldType::KeyToWord)
                            .then_some(binding.field_column[i])
                            .flatten()
                    })
                    .collect::<Vec<_>>()
            })
            .collect();
        for (r, row) in table.rows.iter().enumerate() {
            if table.disabled(r) {
                continue;
            }
            for (c, value) in row.cells.iter().enumerate() {
                if value.is_empty()
                    || before
                        .and_then(|t| t.rows.get(r))
                        .and_then(|r| r.cells.get(c))
                        == Some(value)
                {
                    continue;
                }
                let column = String::from_utf8_lossy(&table.header[c]);
                let label = format!(
                    "{}.txt row #{r} ({}) column `{column}` value `{}`",
                    table.name(),
                    String::from_utf8_lossy(table.key(r)),
                    String::from_utf8_lossy(value)
                );
                if str_cols.contains(&c) && strings.lookup(value).is_none() {
                    errors.push(format!("{label}: missing string key"));
                }
                let paths = paths(table.name(), &column.to_ascii_lowercase(), value, r);
                if paths.is_empty() {
                    continue;
                }
                let max_len = schema()
                    .called()
                    .filter(|d| d.txt_name == table.rules.txt_name)
                    .flat_map(|d| &d.fields)
                    .filter(|f| {
                        f.field_type == FieldType::Ascii
                            && f.column.eq_ignore_ascii_case(&table.header[c])
                    })
                    .map(|f| f.length as usize)
                    .min()
                    .or_else(|| {
                        (table.name() == "sounds" && column.eq_ignore_ascii_case("filename"))
                            .then_some(d2_data::sounds::FILE_NAME_LEN as usize)
                    });
                if let Some(max) = max_len.filter(|&max| value.len() > max) {
                    errors.push(format!(
                        "{label}: asset filename exceeds {max} bytes and would be truncated"
                    ));
                    continue;
                }
                if paths.iter().any(|p| {
                    p.contains(':') || p.split('/').any(|s| s.is_empty() || s == "." || s == "..")
                }) {
                    errors.push(format!(
                        "{label}: asset path must be relative without . or .. components"
                    ));
                    continue;
                }
                let Some(path) = paths.iter().find(|p| src.contains(p)) else {
                    errors.push(format!("{label}: missing asset {}", paths.join(" or ")));
                    continue;
                };
                if let Some(Err(e)) = src.read_native(path) {
                    errors.push(format!("{label}: {e}"));
                }
            }
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("; "))
    }
}

/// Explicit file columns, using the same roots as the consuming loaders.
/// Token-derived animation combinations are checked by the animation sweeps.
fn paths(table: &str, column: &str, value: &[u8], row: usize) -> Vec<String> {
    let value = fold(&String::from_utf8_lossy(value));
    let one = |p: String| vec![p];
    match (table, column) {
        (
            "weapons" | "armor" | "misc" | "uniqueitems" | "setitems",
            "invfile" | "flippyfile" | "uniqueinvfile" | "setinvfile",
        ) => one(format!("data/global/items/{value}.dc6")),
        ("itemtypes", c) if (1..=6).any(|i| c == format!("invgfx{i}")) => {
            one(format!("data/global/items/{value}.dc6"))
        }
        ("sounds", "filename") => one(fold(&d2_data::sounds::sound_path(
            row as i32,
            value.as_bytes(),
        ))),
        ("lvlprest", c) if (1..=6).any(|i| c == format!("file{i}")) => {
            if value == "0" {
                Vec::new()
            } else {
                one(format!("data/global/tiles/{value}"))
            }
        }
        ("lvltypes", c) if (1..=32).any(|i| c == format!("file {i}")) => {
            one(format!("data/global/tiles/{value}"))
        }
        ("lvlsub", "file") => one(format!("data/global/tiles/{value}")),
        ("missiles", "celfile") => ["dcc", "dc6"]
            .map(|ext| format!("data/global/missiles/{value}.{ext}"))
            .into(),
        ("overlay", "filename") => ["dcc", "dc6"]
            .map(|ext| format!("data/global/overlays/{value}.{ext}"))
            .into(),
        _ => Vec::new(),
    }
}

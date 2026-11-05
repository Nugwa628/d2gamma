//! Effect documents and Studio preview, using the shared runtime recipe schema.
use anyhow::{bail, Context, Result};
use d2_modkit::effects::Effect;
use eframe::egui::{self, Color32};
use std::path::{Path, PathBuf};
use toml::{Table, Value};
struct Document {
    path: PathBuf,
    table: Table,
    dirty: bool,
}
#[derive(Default)]
pub struct EffectsEditor {
    root: Option<PathBuf>,
    documents: Vec<Document>,
    selected: usize,
    effect: usize,
    error: String,
    load_error: bool,
    raw: String,
    clips: Vec<Clip>,
    sprite_key: String,
    palette: Option<d2_native::kind::ViewPalette>,
    palette_root: Option<String>,
    preview_open: bool,
}
struct Clip {
    frames: Vec<crate::sprite_preview::Frame>,
    texture: Option<egui::TextureHandle>,
}
impl EffectsEditor {
    pub fn open(&mut self, root: &Path) {
        *self = Self::default();
        self.root = Some(root.into());
        let result = {
            fn scan(dir: &Path, out: &mut Vec<Document>) -> Result<()> {
                if !dir.exists() {
                    return Ok(());
                }
                let mut entries = std::fs::read_dir(dir)?.collect::<std::io::Result<Vec<_>>>()?;
                entries.sort_by_key(|e| e.file_name());
                for e in entries {
                    let kind = e.file_type()?;
                    if kind.is_dir() {
                        scan(&e.path(), out)?;
                    } else if kind.is_file() && e.path().extension().is_some_and(|x| x == "toml") {
                        if out.len() >= 64 || e.metadata()?.len() > 256 * 1024 {
                            bail!("At most 64 effect files, 256 KiB each");
                        }
                        let text = std::fs::read_to_string(e.path())?;
                        d2_modkit::effects::parse(&text).map_err(anyhow::Error::msg)?;
                        out.push(Document {
                            path: e.path(),
                            table: text.parse()?,
                            dirty: false,
                        });
                    }
                }
                Ok(())
            }
            scan(&root.join("files/effects"), &mut self.documents)
        };
        if let Err(e) = result {
            self.error = format!("{e:#}");
            self.load_error = true;
            self.documents.clear();
        }
    }
    pub fn dirty(&self) -> bool {
        self.documents.iter().any(|d| d.dirty)
    }
    pub fn save(&mut self) -> Result<()> {
        if self.load_error {
            bail!("{}", self.error);
        }
        let mut names = std::collections::BTreeSet::new();
        let mut writes = Vec::new();
        for (i, d) in self.documents.iter().enumerate() {
            let text = toml::to_string_pretty(&d.table)?;
            for e in d2_modkit::effects::parse(&text).map_err(anyhow::Error::msg)? {
                if !names.insert(e.name) {
                    bail!("Duplicate effect name");
                }
                if names.len() > 512 {
                    bail!("At most 512 effects");
                }
            }
            if d.dirty {
                writes.push((i, text));
            }
        }
        for (i, text) in writes {
            let d = &mut self.documents[i];
            std::fs::create_dir_all(d.path.parent().context("No effect parent")?)?;
            std::fs::write(&d.path, text)?;
            d.dirty = false;
        }
        Ok(())
    }
    pub fn add(&mut self) -> Result<()> {
        let root = self.root.as_ref().context("Open a project")?;
        if self.documents.is_empty() {
            let path = root.join("files/effects/studio.toml");
            if path.exists() {
                bail!("Reopen project to load existing effect file");
            }
            self.documents.push(Document {
                path,
                table: "version=1".parse()?,
                dirty: true,
            });
            self.selected = 0;
        }
        let d = self
            .documents
            .get_mut(self.selected)
            .context("Choose an effect file")?;
        let a = d
            .table
            .entry("effects")
            .or_insert_with(|| Value::Array(Vec::new()))
            .as_array_mut()
            .context("effects must be an array")?;
        if a.len() >= 128 {
            bail!("At most 128 effects per file");
        }
        let mut n = 1;
        let name = loop {
            let name = format!("effect_{n}");
            if !a
                .iter()
                .any(|e| e.get("name").and_then(Value::as_str) == Some(&name))
            {
                break name;
            }
            n += 1;
        };
        let t: Table =
            format!("name='{name}'\nduration=25\n[particles]\ncount=16\ncolor=255").parse()?;
        a.push(Value::Table(t));
        self.effect = a.len() - 1;
        d.dirty = true;
        self.raw.clear();
        Ok(())
    }
    pub fn draw(
        &mut self,
        ui: &mut egui::Ui,
        tables: Option<&crate::tables::Tables>,
        native: &str,
    ) {
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("effect-document")
                .selected_text(
                    self.documents
                        .get(self.selected)
                        .and_then(|d| d.path.file_name())
                        .map_or("Effect file".into(), |n| n.to_string_lossy()),
                )
                .show_ui(ui, |ui| {
                    for (i, d) in self.documents.iter().enumerate() {
                        if ui
                            .selectable_value(&mut self.selected, i, d.path.display().to_string())
                            .changed()
                        {
                            self.raw.clear();
                        }
                    }
                });
            ui.checkbox(&mut self.preview_open, "Show preview");
            if ui.button("New effect").clicked() {
                self.error = self
                    .add()
                    .err()
                    .map_or_else(String::new, |e| format!("{e:#}"));
            }
            if ui.button("Save effects").clicked() {
                self.error = self
                    .save()
                    .err()
                    .map_or_else(String::new, |e| format!("{e:#}"));
            }
        });
        ui.weak("Visual recipes use existing missile/overlay art, short-lived lights and indexed particles. The server decides when they play.");
        if !self.error.is_empty() {
            ui.colored_label(Color32::LIGHT_RED, &self.error);
        }
        let Some(d) = self.documents.get_mut(self.selected) else {
            return;
        };
        let mut changed = false;
        let mut recipe = None;
        egui::ScrollArea::vertical()
            .max_height(
                (ui.available_height() - if self.preview_open { 400. } else { 0. }).max(160.),
            )
            .show(ui, |ui| {
                let a = d.table.get_mut("effects").and_then(Value::as_array_mut);
                let Some(a) = a else {
                    return;
                };
                egui::ComboBox::from_id_salt("effect-recipe")
                    .selected_text(
                        a.get(self.effect)
                            .and_then(|v| v.get("name"))
                            .and_then(Value::as_str)
                            .unwrap_or("Choose effect"),
                    )
                    .show_ui(ui, |ui| {
                        for (i, v) in a.iter().enumerate() {
                            ui.selectable_value(
                                &mut self.effect,
                                i,
                                v.get("name").and_then(Value::as_str).unwrap_or("?"),
                            );
                        }
                    });
                if ui.button("Delete selected effect").clicked() && self.effect < a.len() {
                    a.remove(self.effect);
                    self.effect = self.effect.saturating_sub(1);
                    changed = true;
                }
                if let Some(e) = a.get_mut(self.effect).and_then(Value::as_table_mut) {
                    changed |= string(ui, e, "name", "Name");
                    changed |= number(ui, e, "duration", "Ticks", 25, 1, 3600);
                    for (key, table, column) in [
                        ("missile", "missiles", "Missile"),
                        ("overlay", "overlay", "overlay"),
                    ] {
                        changed |= string(ui, e, key, key);
                        let names = table_names(tables, table, column);
                        egui::ComboBox::from_id_salt(("effect-art", key))
                            .height(250.)
                            .selected_text(format!("Pick {key}"))
                            .show_ui(ui, |ui| {
                                for n in names {
                                    if ui.selectable_label(false, &n).clicked() {
                                        e.insert(key.into(), Value::String(n));
                                        changed = true;
                                    }
                                }
                            });
                    }
                    for key in ["light", "particles"] {
                        let mut enabled = e.contains_key(key);
                        if ui.checkbox(&mut enabled, key).changed() {
                            if enabled {
                                e.insert(key.into(), Value::Table(Table::new()));
                            } else {
                                e.remove(key);
                            }
                            changed = true;
                        }
                    }
                    if let Some(l) = e.get_mut("light").and_then(Value::as_table_mut) {
                        for (k, label, default, min, max) in [
                            ("radius", "Radius", 8, 1, 18),
                            ("red", "Red", 255, 0, 255),
                            ("green", "Green", 180, 0, 255),
                            ("blue", "Blue", 80, 0, 255),
                        ] {
                            changed |= number(ui, l, k, label, default, min, max);
                        }
                    }
                    if let Some(p) = e.get_mut("particles").and_then(Value::as_table_mut) {
                        for (k, label, default, min, max) in [
                            ("count", "Count", 16, 1, 128),
                            ("speed", "Speed", 4, 0, 64),
                            ("spread", "Spread", 32, 0, 512),
                            ("gravity", "Gravity", 0, -64, 64),
                            ("color", "Color index", 255, 1, 255),
                        ] {
                            changed |= number(ui, p, k, label, default, min, max);
                        }
                    }
                }
                d.dirty |= changed;
                match d2_modkit::effects::parse(&toml::to_string(&d.table).unwrap_or_default()) {
                    Ok(e) => recipe = e.get(self.effect).cloned(),
                    Err(e) => {
                        ui.colored_label(Color32::LIGHT_RED, e);
                    }
                }
                egui::CollapsingHeader::new("Advanced effect TOML").show(ui, |ui| {
                    if self.raw.is_empty() || ui.button("Read current document").clicked() {
                        self.raw = toml::to_string_pretty(&d.table).unwrap_or_default();
                    }
                    ui.add(
                        egui::TextEdit::multiline(&mut self.raw)
                            .font(egui::TextStyle::Monospace)
                            .desired_width(f32::INFINITY)
                            .desired_rows(8),
                    );
                    if ui.button("Apply document").clicked() {
                        match d2_modkit::effects::parse(&self.raw) {
                            Ok(_) => match self.raw.parse() {
                                Ok(t) => {
                                    d.table = t;
                                    d.dirty = true;
                                    self.error.clear();
                                }
                                Err(e) => self.error = format!("{e}"),
                            },
                            Err(e) => self.error = e,
                        }
                    }
                });
            });
        if self.preview_open {
            if let Some(recipe) = recipe {
                self.preview(ui, &recipe, tables, native);
            }
        }
    }
    fn preview(
        &mut self,
        ui: &mut egui::Ui,
        e: &Effect,
        tables: Option<&crate::tables::Tables>,
        native: &str,
    ) {
        if self.palette_root.as_deref() != Some(native) {
            self.palette_root = Some(native.into());
            self.palette =
                std::fs::read(Path::new(native).join("base/data/global/palette/act1/pal.dat.pal"))
                    .ok()
                    .and_then(|b| crate::import::palette(b).ok());
        }
        let age = (ui.input(|i| i.time) * 25.) as u64 % u64::from(e.duration);
        ui.horizontal(|ui| {
            ui.heading("Preview");
            ui.label(format!("tick {age}/{}", e.duration));
        });
        let available = ui.available_width().min(900.);
        let (r, _) = ui.allocate_exact_size(egui::vec2(available, 300.), egui::Sense::hover());
        let painter = ui.painter_at(r);
        painter.rect_filled(r, 0., Color32::from_gray(18));
        let center = r.center();
        if let Some(l) = &e.light {
            let radius = f32::from(l.radius) * 8.;
            painter.circle_filled(
                center,
                radius,
                Color32::from_rgba_unmultiplied(l.red, l.green, l.blue, 40),
            );
            painter.circle_stroke(
                center,
                radius,
                egui::Stroke::new(1_f32, Color32::from_rgb(l.red, l.green, l.blue)),
            );
        }
        if let Some(p) = &e.particles {
            for i in 0..p.count {
                let (x, y) = d2_modkit::effects::particle_position(p, i, age as u16);
                painter.circle_filled(
                    center + egui::vec2(x as f32, y as f32),
                    1.5,
                    self.palette
                        .as_ref()
                        .map_or(Color32::from_gray(p.color), |palette| {
                            let c = palette[usize::from(p.color)];
                            Color32::from_rgb(c.r, c.g, c.b)
                        }),
                );
            }
        }
        if let Some(root) = &self.root {
            let paths: Vec<_> = e
                .missile
                .as_ref()
                .and_then(|n| art_path(tables, "missiles", "Missile", "CelFile", n, "missiles"))
                .into_iter()
                .chain(e.overlay.as_ref().and_then(|n| {
                    art_path(tables, "overlay", "overlay", "Filename", n, "overlays")
                }))
                .collect();
            let key = paths.join("|");
            if !paths.is_empty() {
                if ui.button("Load sprite preview").clicked() {
                    self.palette = std::fs::read(
                        Path::new(native).join("base/data/global/palette/act1/pal.dat.pal"),
                    )
                    .ok()
                    .and_then(|b| crate::import::palette(b).ok());
                    self.clips.clear();
                    self.sprite_key = key.clone();
                    for path in &paths {
                        match crate::sprite_preview::frames(
                            root,
                            Path::new(native),
                            &format!("{path}.dcc"),
                        )
                        .or_else(|_| {
                            crate::sprite_preview::frames(
                                root,
                                Path::new(native),
                                &format!("{path}.dc6"),
                            )
                        }) {
                            Ok(frames) => self.clips.push(Clip {
                                frames,
                                texture: None,
                            }),
                            Err(e) => self.error = format!("Sprite preview: {e:#}"),
                        }
                    }
                }
                if self.sprite_key == key {
                    let palette = self.palette.unwrap_or_else(d2_native::kind::grey_palette);
                    for (i, clip) in self.clips.iter_mut().enumerate() {
                        if clip.frames.is_empty() {
                            continue;
                        }
                        let f = &clip.frames[age as usize % clip.frames.len()];
                        let image = crate::sprite_preview::color(f, &palette);
                        let t = clip.texture.get_or_insert_with(|| {
                            ui.ctx().load_texture(
                                format!("effect-sprite-{i}"),
                                image.clone(),
                                egui::TextureOptions::NEAREST,
                            )
                        });
                        t.set(image, egui::TextureOptions::NEAREST);
                        let target = egui::Rect::from_min_size(
                            center + egui::vec2(f.x as f32, f.y as f32),
                            egui::vec2(f.width as f32, f.height as f32),
                        );
                        painter.image(
                            t.id(),
                            target,
                            egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1., 1.)),
                            Color32::WHITE,
                        );
                    }
                }
            } else if e.missile.is_some() || e.overlay.is_some() {
                ui.weak("Load tables to resolve named art. Missing art remains visible as a recipe error in Check.");
            }
        }
        ui.weak("Palette indices use the selected install's act 1 palette for sprite preview; missing palette uses gray. Light circles show radius/color; sprites preview one frame per tick. In-game shading and occlusion use the engine.");
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(40));
    }
}
fn string(ui: &mut egui::Ui, t: &mut Table, k: &str, label: &str) -> bool {
    let mut v = crate::layout::text(t, k).to_owned();
    let changed = ui
        .horizontal(|ui| {
            ui.label(label);
            ui.text_edit_singleline(&mut v).changed()
        })
        .inner;
    if changed {
        if v.is_empty() {
            t.remove(k);
        } else {
            t.insert(k.into(), Value::String(v));
        }
    }
    changed
}
fn number(
    ui: &mut egui::Ui,
    t: &mut Table,
    k: &str,
    label: &str,
    d: i64,
    min: i64,
    max: i64,
) -> bool {
    let mut v = crate::layout::integer(t, k, d);
    let changed = ui
        .horizontal(|ui| {
            ui.label(label);
            ui.add(egui::DragValue::new(&mut v).range(min..=max))
                .changed()
        })
        .inner;
    if changed {
        t.insert(k.into(), Value::Integer(v));
    }
    changed
}
fn table_names(tables: Option<&crate::tables::Tables>, table: &str, column: &str) -> Vec<String> {
    let Some(t) = tables.and_then(|t| {
        t.edited
            .tables
            .iter()
            .find(|t| t.name().eq_ignore_ascii_case(table))
    }) else {
        return Vec::new();
    };
    let Some(c) = t
        .header
        .iter()
        .position(|h| h.eq_ignore_ascii_case(column.as_bytes()))
    else {
        return Vec::new();
    };
    t.rows
        .iter()
        .enumerate()
        .filter(|(i, _)| !t.disabled(*i))
        .filter_map(|(_, r)| {
            let n = crate::tables::text(&r.cells[c]);
            (!n.is_empty()).then_some(n)
        })
        .collect()
}
fn art_path(
    tables: Option<&crate::tables::Tables>,
    table: &str,
    key: &str,
    file: &str,
    name: &str,
    folder: &str,
) -> Option<String> {
    let t = tables?
        .edited
        .tables
        .iter()
        .find(|t| t.name().eq_ignore_ascii_case(table))?;
    let k = t
        .header
        .iter()
        .position(|h| h.eq_ignore_ascii_case(key.as_bytes()))?;
    let f = t
        .header
        .iter()
        .position(|h| h.eq_ignore_ascii_case(file.as_bytes()))?;
    let row = t
        .rows
        .iter()
        .find(|r| r.cells[k].eq_ignore_ascii_case(name.as_bytes()))?;
    let file = crate::tables::text(&row.cells[f]);
    (!file.is_empty()).then(|| format!("data/global/{folder}/{file}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn effects_editor_roundtrip_global_validation_and_default_empty() {
        let root = std::env::temp_dir().join(format!("effects-editor-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let mut e = EffectsEditor::default();
        e.open(&root);
        e.save().unwrap();
        assert!(!root.join("files/effects").exists());
        e.add().unwrap();
        assert!(e.dirty());
        e.save().unwrap();
        assert!(!e.dirty());
        e.open(&root);
        assert_eq!(e.documents.len(), 1);
        let path = root.join("files/effects/second.toml");
        std::fs::write(&path, std::fs::read(&e.documents[0].path).unwrap()).unwrap();
        e.open(&root);
        e.documents[0].dirty = true;
        assert!(e.save().is_err());
        assert!(e.dirty());
        std::fs::write(path, "version=2").unwrap();
        e.open(&root);
        assert!(e.save().is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn effect_properties_fit_without_the_optional_preview() {
        let mut e = EffectsEditor::default();
        e.open(&std::env::temp_dir().join("effect-properties-viewport"));
        e.add().unwrap();
        let effect = e.documents[0].table["effects"].as_array_mut().unwrap()[0]
            .as_table_mut()
            .unwrap();
        effect.insert("light".into(), Value::Table(Table::new()));
        let ctx = egui::Context::default();
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(850., 660.),
                )),
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| e.draw(ui, None, ""));
                let text = crate::input_script::painted_text(ctx);
                for label in [
                    "Name",
                    "Radius",
                    "Red",
                    "Green",
                    "Blue",
                    "Count",
                    "Speed",
                    "Spread",
                    "Gravity",
                    "Color index",
                ] {
                    assert!(text.iter().any(|p| p.text == label), "{label} is clipped");
                }
                assert!(!text.iter().any(|p| p.text == "Preview"));
            },
        );
    }
    #[test]
    fn effects_editor_draws_shared_light_and_particle_preview() {
        let root = std::env::temp_dir().join(format!("effects-preview-{}", std::process::id()));
        let mut e = EffectsEditor {
            preview_open: true,
            ..Default::default()
        };
        e.open(&root);
        e.add().unwrap();
        let context = egui::Context::default();
        let output = context.run(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| e.draw(ui, None, ""));
        });
        assert!(!output.shapes.is_empty());
    }
}

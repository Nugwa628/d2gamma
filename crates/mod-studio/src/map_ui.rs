//! Map painting, typed placements and bounded batches of simulation seed previews.
use crate::maps::{
    editor::{pick_tile, Document, Layer},
    MapProject,
};
use anyhow::{Context, Result};
use d2_native::map::{self, TileLibrary};
use d2_sim::drlg::maps::{generation::Generation, Level, Object, Spawn, Stamp, Warp};
use eframe::egui::{self, Color32, TextureHandle};
use std::{path::Path, sync::mpsc};
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Tool {
    #[default]
    Paint,
    Erase,
    Entry,
    Warp,
    Object,
    Spawn,
    Preset,
}
struct Thumbnail {
    level: u32,
    seed: u32,
    w: usize,
    h: usize,
    rgba: Vec<u8>,
    texture: Option<TextureHandle>,
}
type Batch = (u64, mpsc::Receiver<Result<Vec<Thumbnail>, String>>);
struct Brush {
    path: String,
    index: usize,
    key: map::TileKey,
}
impl Brush {
    fn label(&self) -> String {
        format!(
            "{} [{}]: {}:{} / {}",
            self.path, self.index, self.key.main, self.key.sub, self.key.orientation
        )
    }
}
fn brushes(project: &MapProject, level: &Level, wall: bool) -> Vec<Brush> {
    let mut out = Vec::new();
    for path in &level.tiles {
        if let Some(dt1) = project.tiles.get(path) {
            for (index, tile) in dt1.tiles.iter().enumerate() {
                if (tile.orientation != 0) == wall {
                    out.push(Brush {
                        path: path.clone(),
                        index,
                        key: map::TileKey {
                            orientation: tile.orientation,
                            main: tile.main_index,
                            sub: tile.sub_index,
                        },
                    });
                }
            }
        }
    }
    out
}
#[derive(Default)]
pub struct MapEditor {
    pub doc: Option<Document>,
    message: String,
    level: usize,
    variant: usize,
    wall: bool,
    layer: usize,
    brush: usize,
    tool: Tool,
    seed: u32,
    count: usize,
    previews: Vec<Thumbnail>,
    preview_revision: u64,
    batch: Option<Batch>,
    texture: Option<TextureHandle>,
    frame_key: String,
    scene_key: String,
    scene: Option<(map::Layout, TileLibrary)>,
    scene_error: String,
    validation_revision: Option<u64>,
    validation: String,
    pan: [i32; 2],
    zoom: i8,
    realized: bool,
    revision: u64,
    copy_id: u32,
    import: String,
    first_id: u32,
    first_map: String,
    first_tiles: String,
    preset: String,
    class: u16,
    pixel: u8,
}
impl MapEditor {
    pub fn dirty(&self) -> bool {
        self.doc.as_ref().is_some_and(|d| d.dirty)
    }
    pub fn save(&mut self) -> Result<()> {
        if let Some(d) = &mut self.doc {
            d.save()?;
        }
        Ok(())
    }
    fn result(&mut self, result: Result<()>) {
        match result {
            Ok(()) => {
                self.revision += 1;
                self.message.clear();
            }
            Err(e) => self.message = format!("{e:#}"),
        }
    }
    pub fn draw(
        &mut self,
        ui: &mut egui::Ui,
        root: &Path,
        native: &Path,
        installed: &Path,
        screen_only: bool,
    ) {
        if self.count == 0 {
            self.count = 32;
            self.copy_id = 2;
        }
        if screen_only {
            ui.label("Maps require a full mod or a gameplay add-on.");
            return;
        }
        if self.doc.is_none() {
            if self.first_id == 0 {
                self.first_id = 137;
            }
            ui.group(|ui| {
                ui.label("Start from imported map art");
                ui.horizontal(|ui| {
                    ui.label("First level ID");
                    ui.add(egui::DragValue::new(&mut self.first_id).range(1..=255));
                });
                ui.horizontal(|ui| {
                    ui.label("Source DS1");
                    ui.text_edit_singleline(&mut self.first_map);
                });
                ui.horizontal(|ui| {
                    ui.label("Source DT1");
                    ui.text_edit_singleline(&mut self.first_tiles);
                });
                if ui.button("Create level from project art").clicked() {
                    match Document::from_project_assets(
                        root,
                        self.first_id,
                        &self.first_map,
                        &self.first_tiles,
                    ) {
                        Ok(doc) => {
                            self.doc = Some(doc);
                            self.level = 0;
                            self.variant = 0;
                            self.zoom = -2;
                            self.pan = [-2560, 0];
                            self.revision += 1;
                            self.message.clear();
                        }
                        Err(e) => self.message = format!("{e:#}"),
                    }
                }
            });
        }
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    !self.dirty() && self.batch.is_none(),
                    egui::Button::new("Load / reload maps"),
                )
                .clicked()
            {
                let result = MapProject::load(root, native, installed).and_then(Document::new);
                match result {
                    Ok(d) => {
                        self.doc = Some(d);
                        self.level = 0;
                        self.variant = 0;
                        self.zoom = -2;
                        self.pan = [-2560, 0];
                        self.previews.clear();
                        self.revision += 1;
                        self.message.clear();
                    }
                    Err(e) => self.message = format!("{e:#}"),
                }
            }
            if ui
                .add_enabled(
                    self.doc.is_none(),
                    egui::Button::new("New map + generated tiles"),
                )
                .clicked()
            {
                match Document::starter(root) {
                    Ok(d) => {
                        self.doc = Some(d);
                        self.zoom = -2;
                        self.pan = [-2560, 0];
                        self.revision += 1;
                        self.message.clear();
                    }
                    Err(e) => self.message = format!("{e:#}"),
                }
            }
            if ui.button("Save maps").clicked() {
                let r = self.save();
                self.result(r);
            }
            if ui.button("Undo").clicked() {
                let r = self.doc.as_mut().map_or(Ok(()), Document::undo);
                self.result(r);
            }
            if ui.button("Redo").clicked() {
                let r = self.doc.as_mut().map_or(Ok(()), Document::redo);
                self.result(r);
            }
            if self.dirty() {
                ui.colored_label(Color32::YELLOW, "Unsaved map edits");
            }
        });
        if !self.message.is_empty() {
            ui.colored_label(Color32::LIGHT_RED, &self.message);
        }
        let Some(doc) = &mut self.doc else {
            ui.label("Load files/levels.toml, or start with generated tiles. Set the converted install path above to load existing assets.");
            return;
        };
        self.level = self.level.min(doc.project.maps.level.len() - 1);
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("map-level")
                .selected_text(format!("Level {}", doc.project.maps.level[self.level].id))
                .show_ui(ui, |ui| {
                    for (i, l) in doc.project.maps.level.iter().enumerate() {
                        if ui
                            .selectable_value(
                                &mut self.level,
                                i,
                                format!("Level {} · {}×{}", l.id, l.size[0], l.size[1]),
                            )
                            .changed()
                        {
                            self.variant = 0;
                            self.previews.clear();
                            self.revision += 1;
                        }
                    }
                });
            ui.label("Clone to id");
            ui.add(egui::DragValue::new(&mut self.copy_id).range(1..=255));
            if ui
                .button("Add level")
                .on_hover_text(
                    "Shares source variants; import a separate DS1 to paint independently",
                )
                .clicked()
            {
                let r = doc.add_level(doc.project.maps.level[self.level].id, self.copy_id);
                if let Err(e) = r {
                    self.message = e.to_string();
                } else {
                    self.revision += 1;
                }
            }
            ui.label("Source variant");
            let len = doc.project.maps.level[self.level].files.len();
            ui.add(egui::DragValue::new(&mut self.variant).range(0..=len.saturating_sub(1)));
        });
        let current = doc.project.maps.level[self.level].clone();
        let path = current.files[self.variant.min(current.files.len() - 1)].clone();
        egui::CollapsingHeader::new("Level rules and placements").show(ui,|ui| {
            let mut l=current.clone();let paths:Vec<_>=doc.project.files.decoded.keys().map(|p|String::from_utf8_lossy(p).into_owned()).collect();level_form(ui,&mut l,&paths);
            if l!=current {if let Err(e)=doc.update_level(l) {self.message=e.to_string();}self.revision+=1;}
            ui.horizontal(|ui| {ui.label("Project DS1 path");ui.text_edit_singleline(&mut self.import);if ui.button("Import sidecar").clicked() {match doc.import_map(&self.import) {Ok(())=>{self.revision+=1;self.message.clear();},Err(e)=>self.message=e.to_string()}}});
            ui.weak("Paths are relative to data/global/tiles. New IDs also need table rows; cross-act travel is configured in acts.toml.");
        });
        if self.validation_revision != Some(self.revision) {
            self.validation = doc
                .validate()
                .err()
                .map_or(String::new(), |e| e.to_string());
            self.validation_revision = Some(self.revision);
        }
        if !self.validation.is_empty() {
            ui.colored_label(Color32::LIGHT_RED, &self.validation);
        }
        ui.horizontal(|ui| {
            if ui.checkbox(&mut self.wall, "Wall layer").changed() {
                self.brush = 0;
            }
        });
        let keys = brushes(&doc.project, &current, self.wall);
        ui.horizontal(|ui| {
            for (t, label) in [
                (Tool::Paint, "Paint"),
                (Tool::Erase, "Erase"),
                (Tool::Entry, "Entry"),
                (Tool::Warp, "Warp"),
                (Tool::Object, "Object"),
                (Tool::Spawn, "Spawn region"),
                (Tool::Preset, "Preset"),
            ] {
                ui.selectable_value(&mut self.tool, t, label);
            }
        });
        ui.horizontal(|ui| {
            let d = &doc.project.files.decoded[path.as_bytes()];
            let max = if self.wall {
                d.walls.len()
            } else {
                d.floors.len()
            };
            self.layer = self.layer.min(max.saturating_sub(1));
            ui.add(egui::DragValue::new(&mut self.layer).range(0..=max.saturating_sub(1)));
            if ui.button("Add layer").clicked() {
                if let Err(e) = doc.add_layer(&path, self.wall) {
                    self.message = e.to_string();
                }
                self.revision += 1;
            }
            self.brush = self.brush.min(keys.len().saturating_sub(1));
            egui::ComboBox::from_id_salt("map-brush")
                .selected_text(keys.get(self.brush).map_or("No tile".into(), Brush::label))
                .show_ui(ui, |ui| {
                    for (i, k) in keys.iter().enumerate() {
                        ui.selectable_value(&mut self.brush, i, k.label());
                    }
                });
            ui.label("Object/monster class");
            ui.add(egui::DragValue::new(&mut self.class));
            egui::ComboBox::from_id_salt("stamp-brush")
                .selected_text(&self.preset)
                .show_ui(ui, |ui| {
                    for p in doc.project.files.decoded.keys() {
                        let p = String::from_utf8_lossy(p).into_owned();
                        ui.selectable_value(&mut self.preset, p.clone(), p);
                    }
                });
        });
        egui::CollapsingHeader::new("Tile library · pixels and collision").show(ui, |ui| {
            if let Some(key) = keys.get(self.brush) {
                if tile_form(ui, doc, key, &mut self.pixel, &mut self.message) {
                    self.revision += 1;
                }
            }
        });
        let mut fit = false;
        ui.horizontal(|ui| {
            ui.checkbox(&mut self.realized, "Show seeded layout");
            ui.label("Seed");
            ui.add(egui::DragValue::new(&mut self.seed));
            ui.label("Zoom");
            ui.add(egui::DragValue::new(&mut self.zoom).range(-10..=2));
            if ui.button("Fit").clicked() {
                fit = true;
            }
            ui.weak("Paint in source view. Pan with middle/right drag; wheel to zoom.");
        });
        let scene_key = format!(
            "{}:{}:{}:{}:{}",
            self.revision, self.level, self.variant, self.seed, self.realized
        );
        if scene_key != self.scene_key {
            let mut library = TileLibrary::new();
            for name in &current.tiles {
                if let Some(d) = doc.project.tiles.get(name) {
                    library.add_dt1(d);
                }
            }
            let decoded = if self.realized {
                doc.project
                    .preview(current.id, self.seed, 1)
                    .and_then(|p| doc.project.decoded_preview(&p[0]))
            } else {
                Ok(doc.project.files.decoded[path.as_bytes()].clone())
            };
            match decoded {
                Ok(ds1) => {
                    self.scene = Some((map::build(&ds1, &library, 0), library));
                    self.scene_error.clear();
                }
                Err(e) => {
                    self.scene = None;
                    self.scene_error = e.to_string();
                }
            }
            self.scene_key = scene_key;
        }
        if let Some((layout, library)) = &self.scene {
            if !layout.missing.is_empty() {
                ui.colored_label(
                    Color32::YELLOW,
                    format!(
                        "{} tile keys missing from the level libraries",
                        layout.missing.len()
                    ),
                );
            }
            let size = egui::vec2(
                ui.available_width().clamp(200.0, 1800.0),
                ui.available_height().clamp(180.0, 360.0),
            );
            let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click_and_drag());
            if fit {
                if let Some(bounds) = layout.bounds {
                    self.pan = [bounds.x0, bounds.y0];
                    self.zoom = 2;
                    while self.zoom > -10
                        && ((bounds.x1 - bounds.x0) as f32 * 2.0f32.powi(i32::from(self.zoom))
                            > size.x
                            || (bounds.y1 - bounds.y0) as f32 * 2.0f32.powi(i32::from(self.zoom))
                                > size.y)
                    {
                        self.zoom -= 1;
                    }
                }
            }
            let scale = 2.0f32.powi(i32::from(self.zoom));
            if response.dragged_by(egui::PointerButton::Middle)
                || response.dragged_by(egui::PointerButton::Secondary)
            {
                let delta = ui.input(|i| i.pointer.delta());
                self.pan[0] -= (delta.x / scale) as i32;
                self.pan[1] -= (delta.y / scale) as i32;
            }
            if response.hovered() {
                let scroll = ui.input(|i| i.smooth_scroll_delta.y);
                if scroll.abs() > 1.0 {
                    self.zoom = (self.zoom + if scroll > 0.0 { 1 } else { -1 }).clamp(-10, 2);
                }
            }
            let scale = 2.0f32.powi(i32::from(self.zoom));
            let key = format!(
                "{}:{}:{}:{}:{}:{:?}:{}:{:?}",
                self.revision,
                self.level,
                self.variant,
                self.seed,
                self.realized,
                self.pan,
                self.zoom,
                size
            );
            if key != self.frame_key {
                let indices = map::cpu::render_zoomed(
                    layout,
                    library,
                    map::cpu::View {
                        left: self.pan[0],
                        top: self.pan[1],
                        width: size.x as u32,
                        height: size.y as u32,
                    },
                    self.zoom,
                );
                let image = egui::ColorImage::from_rgba_unmultiplied(
                    [size.x as usize, size.y as usize],
                    &grey(&indices),
                );
                self.texture = Some(ui.ctx().load_texture(
                    "map-canvas",
                    image,
                    egui::TextureOptions::NEAREST,
                ));
                self.frame_key = key;
            }
            if let Some(texture) = &self.texture {
                ui.painter().image(
                    texture.id(),
                    rect,
                    egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                    Color32::WHITE,
                );
            }
            overlay(ui, &current, rect, self.pan, scale);
            if let Some(pointer) = response.interact_pointer_pos() {
                if let Some(at) = pick_tile(
                    (pointer.x - rect.left()) / scale + self.pan[0] as f32,
                    (pointer.y - rect.top()) / scale + self.pan[1] as f32,
                ) {
                    let inside = at[0] < current.size[0] && at[1] < current.size[1];
                    if inside
                        && !self.realized
                        && response.hovered()
                        && ((matches!(self.tool, Tool::Paint | Tool::Erase)
                            && ui.input(|i| i.pointer.primary_down()))
                            || response.clicked())
                    {
                        let edit = match self.tool {
                            Tool::Paint | Tool::Erase => {
                                let key = keys.get(self.brush).map(|b| b.key);
                                let (value, orientation) = if self.tool == Tool::Erase {
                                    (0, 0)
                                } else {
                                    key.map_or((0, 0), |k| {
                                        (1 | (k.main << 20) | (k.sub << 8), k.orientation)
                                    })
                                };
                                doc.paint(
                                    &path,
                                    if self.wall {
                                        Layer::Wall(self.layer)
                                    } else {
                                        Layer::Floor(self.layer)
                                    },
                                    at,
                                    value,
                                    orientation,
                                )
                            }
                            _ if response.clicked() => {
                                let mut l = current.clone();
                                match self.tool {
                                    Tool::Entry => l.entry = at,
                                    Tool::Warp => {
                                        if let Some(slot) =
                                            (0..8).find(|s| !l.warp.iter().any(|w| w.slot == *s))
                                        {
                                            l.warp.push(Warp {
                                                slot,
                                                to: if l.id == 1 { 2 } else { 1 },
                                                at,
                                            });
                                        }
                                    }
                                    Tool::Object => l.object.push(Object {
                                        class: self.class,
                                        at,
                                        mode: 0,
                                    }),
                                    Tool::Spawn => l.spawn.push(Spawn {
                                        monster: self.class,
                                        rect: [
                                            at[0],
                                            at[1],
                                            4.min(l.size[0] - at[0]),
                                            4.min(l.size[1] - at[1]),
                                        ],
                                        count: 3,
                                    }),
                                    Tool::Preset => l.preset.push(Stamp {
                                        path: self.preset.clone(),
                                        at,
                                    }),
                                    _ => {}
                                }
                                doc.update_level(l)
                            }
                            _ => Ok(()),
                        };
                        if let Err(e) = edit {
                            self.message = e.to_string();
                        }
                        self.revision += 1;
                    }
                    response
                        .clone()
                        .on_hover_text(format!("Tile {}, {}", at[0], at[1]));
                }
            }
        } else {
            ui.colored_label(Color32::LIGHT_RED, &self.scene_error);
        }
        ui.separator();
        ui.heading("Seed previews");
        if self.preview_revision != self.revision {
            self.previews.clear();
            self.preview_revision = self.revision;
        }
        ui.horizontal(|ui| {
            self.count = self.count.clamp(1, 64);
            ui.label("Layouts");
            ui.add(egui::DragValue::new(&mut self.count).range(1..=64));
            if ui
                .add_enabled(self.batch.is_none(), egui::Button::new("Roll layouts"))
                .clicked()
            {
                let project = doc.project.clone();
                let level = current.id;
                let seed = self.seed;
                let count = self.count;
                let (tx, rx) = mpsc::channel();
                self.batch = Some((self.revision, rx));
                std::thread::spawn(move || {
                    let result =
                        thumbnails(&project, level, seed, count).map_err(|e| format!("{e:#}"));
                    let _ = tx.send(result);
                });
            }
            if self.batch.is_some() {
                ui.spinner();
                ui.ctx()
                    .request_repaint_after(std::time::Duration::from_millis(50));
            }
        });
        if let Some((revision, batch)) = &self.batch {
            let revision = *revision;
            match batch.try_recv() {
                Ok(result) => {
                    self.batch = None;
                    match result {
                        Ok(v) => {
                            if revision == self.revision
                                && v.first().is_none_or(|p| p.level == current.id)
                            {
                                self.previews = v;
                                self.message.clear();
                            }
                        }
                        Err(e) => self.message = e,
                    }
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.batch = None;
                    self.message = "Seed preview worker stopped; roll layouts again.".into();
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        egui::ScrollArea::horizontal()
            .id_salt("map-seeds")
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    for t in &mut self.previews {
                        if t.texture.is_none() {
                            t.texture = Some(ui.ctx().load_texture(
                                format!("seed-{}", t.seed),
                                egui::ColorImage::from_rgba_unmultiplied([t.w, t.h], &t.rgba),
                                egui::TextureOptions::NEAREST,
                            ));
                        }
                        ui.vertical(|ui| {
                            let r = ui.add(
                                egui::Image::new((
                                    t.texture.as_ref().unwrap().id(),
                                    egui::vec2(t.w as f32, t.h as f32),
                                ))
                                .sense(egui::Sense::click()),
                            );
                            if r.clicked() {
                                self.seed = t.seed;
                                self.realized = true;
                            }
                            ui.label(format!("Seed {}", t.seed));
                        });
                    }
                });
            });
    }
}
fn grey(indices: &[u8]) -> Vec<u8> {
    indices.iter().flat_map(|&p| [p, p, p, 255]).collect()
}
fn thumbnails(project: &MapProject, level: u32, seed: u32, count: usize) -> Result<Vec<Thumbnail>> {
    let mut library = TileLibrary::new();
    let l = project
        .maps
        .level
        .iter()
        .find(|l| l.id == level)
        .context("Level missing")?;
    for p in &l.tiles {
        if let Some(d) = project.tiles.get(p) {
            library.add_dt1(d);
        }
    }
    let mut out = Vec::new();
    for i in 0..count {
        let seed = seed.wrapping_add(i as u32);
        let p = project.preview(level, seed, 1)?;
        let d = project.decoded_preview(&p[0])?;
        let layout = map::build(&d, &library, 0);
        let b = layout.bounds.context("Map has no drawable tiles")?;
        let mut zoom = 0i8;
        while zoom > -10
            && (((b.x1 - b.x0) as u32).div_ceil(1 << -zoom) > 300
                || ((b.y1 - b.y0) as u32).div_ceil(1 << -zoom) > 180)
        {
            zoom -= 1;
        }
        let div = 1u32 << -zoom;
        let w = ((b.x1 - b.x0) as u32).div_ceil(div).max(1);
        let h = ((b.y1 - b.y0) as u32).div_ceil(div).max(1);
        let indices = map::cpu::render_zoomed(
            &layout,
            &library,
            map::cpu::View {
                left: b.x0,
                top: b.y0,
                width: w,
                height: h,
            },
            zoom,
        );
        out.push(Thumbnail {
            level,
            seed,
            w: w as usize,
            h: h as usize,
            rgba: grey(&indices),
            texture: None,
        });
    }
    Ok(out)
}
fn xy(ui: &mut egui::Ui, label: &str, p: &mut [u16; 2]) {
    ui.label(label);
    for v in p {
        ui.add(egui::DragValue::new(v).range(0..=256));
    }
}
fn level_form(ui: &mut egui::Ui, l: &mut Level, paths: &[String]) {
    ui.horizontal(|ui| {
        ui.label("Declared size (match source DS1)");
        for v in &mut l.size {
            ui.add(egui::DragValue::new(v).range(8..=256));
        }
        xy(ui, "Entry", &mut l.entry);
        ui.label("World offset");
        for v in &mut l.offset {
            ui.add(egui::DragValue::new(v).range(0..=12000));
        }
    });
    ui.horizontal(|ui| {
        let mut kind = match l.generation {
            Generation::Preset => 0,
            Generation::Maze { .. } => 1,
            Generation::Outdoor { .. } => 2,
        };
        let old = kind;
        egui::ComboBox::from_id_salt("map-generation")
            .selected_text(["Preset", "Maze", "Outdoor"][kind])
            .show_ui(ui, |ui| {
                for (i, name) in ["Preset", "Maze", "Outdoor"].iter().enumerate() {
                    ui.selectable_value(&mut kind, i, *name);
                }
            });
        if kind != old {
            l.generation = match kind {
                1 => Generation::Maze {
                    cell: 3,
                    floor: 0x100001,
                    wall: 0x200001,
                    orientation: 1,
                },
                2 => Generation::Outdoor {
                    density: 25,
                    floor: 0x100001,
                    wall: 0x200001,
                    orientation: 1,
                },
                _ => Generation::Preset,
            };
        }
        let brush = match &mut l.generation {
            Generation::Preset => None,
            Generation::Maze {
                cell,
                floor,
                wall,
                orientation,
            } => {
                ui.label("Cell size");
                ui.add(egui::DragValue::new(cell).range(2..=16));
                Some((floor, wall, orientation))
            }
            Generation::Outdoor {
                density,
                floor,
                wall,
                orientation,
            } => {
                ui.label("Obstacle %");
                ui.add(egui::DragValue::new(density).range(0..=80));
                Some((floor, wall, orientation))
            }
        };
        if let Some((floor, wall, orientation)) = brush {
            ui.label("Floor value");
            ui.add(egui::DragValue::new(floor));
            ui.label("Wall value");
            ui.add(egui::DragValue::new(wall));
            ui.label("Orientation");
            ui.add(egui::DragValue::new(orientation).range(1..=9));
        }
    });
    ui.weak("Random rules regenerate the base grids before presets and placements. Preview the seed to see the result.");
    let mut remove = None;
    let variants = l.files.len();
    for (i, p) in l.files.iter_mut().enumerate() {
        ui.push_id(("variant", i), |ui| {
            ui.horizontal(|ui| {
                ui.label(format!("Variant {}", i + 1));
                egui::ComboBox::from_id_salt("path")
                    .selected_text(&*p)
                    .show_ui(ui, |ui| {
                        for path in paths {
                            ui.selectable_value(p, path.clone(), path);
                        }
                    });
                if variants > 1 && ui.small_button("Remove").clicked() {
                    remove = Some(i);
                }
            });
        });
    }
    if let Some(i) = remove {
        l.files.remove(i);
    }
    if l.files.len() < 6 && ui.button("Add variant").clicked() {
        l.files.push(l.files[0].clone());
    }
    for (i, p) in l.tiles.iter_mut().enumerate() {
        ui.push_id(("tilepath", i), |ui| {
            ui.horizontal(|ui| {
                ui.label("Tile library");
                ui.text_edit_singleline(p);
            });
        });
    }
    ui.weak("Place with the canvas tools, or add an exact placement here.");
    ui.horizontal_wrapped(|ui| {
        if l.warp.len() < 8 && ui.button("Add warp").clicked() {
            if let Some(slot) = (0..8).find(|s| !l.warp.iter().any(|w| w.slot == *s)) {
                l.warp.push(Warp {
                    slot,
                    to: l.id,
                    at: l.entry,
                });
            }
        }
        if l.object.len() < 256 && ui.button("Add object").clicked() {
            l.object.push(Object {
                class: 0,
                at: l.entry,
                mode: 0,
            });
        }
        if l.spawn.len() < 256 && ui.button("Add spawn").clicked() {
            l.spawn.push(Spawn {
                monster: 0,
                rect: [l.entry[0], l.entry[1], 1, 1],
                count: 1,
            });
        }
        if l.preset.len() < 64 && !paths.is_empty() && ui.button("Add preset").clicked() {
            l.preset.push(Stamp {
                path: paths[0].clone(),
                at: l.entry,
            });
        }
    });
    let mut remove = None;
    for (i, w) in l.warp.iter_mut().enumerate() {
        ui.push_id(("warp", i), |ui| {
            ui.horizontal(|ui| {
                ui.label("Warp slot");
                ui.add(egui::DragValue::new(&mut w.slot).range(0..=7));
                ui.label("To level");
                ui.add(egui::DragValue::new(&mut w.to).range(1..=255));
                xy(ui, "Warp at", &mut w.at);
                if ui.small_button("Remove warp").clicked() {
                    remove = Some(i);
                }
            });
        });
    }
    if let Some(i) = remove {
        l.warp.remove(i);
    }
    let mut remove = None;
    for (i, o) in l.object.iter_mut().enumerate() {
        ui.push_id(("object", i), |ui| {
            ui.horizontal(|ui| {
                ui.label("Object class");
                ui.add(egui::DragValue::new(&mut o.class));
                xy(ui, "Object at", &mut o.at);
                ui.label("Mode");
                ui.add(egui::DragValue::new(&mut o.mode).range(0..=7));
                if ui.small_button("Remove object").clicked() {
                    remove = Some(i);
                }
            });
        });
    }
    if let Some(i) = remove {
        l.object.remove(i);
    }
    let mut remove = None;
    for (i, s) in l.spawn.iter_mut().enumerate() {
        ui.push_id(("spawn", i), |ui| {
            ui.horizontal(|ui| {
                ui.label("Monster class");
                ui.add(egui::DragValue::new(&mut s.monster));
                ui.label("Region x,y,w,h");
                for v in &mut s.rect {
                    ui.add(egui::DragValue::new(v).range(0..=256));
                }
                ui.label("Count");
                ui.add(egui::DragValue::new(&mut s.count).range(1..=256));
                if ui.small_button("Remove spawn").clicked() {
                    remove = Some(i);
                }
            });
        });
    }
    if let Some(i) = remove {
        l.spawn.remove(i);
    }
    let mut remove = None;
    for (i, p) in l.preset.iter_mut().enumerate() {
        ui.push_id(("preset", i), |ui| {
            ui.horizontal(|ui| {
                ui.label("Preset");
                egui::ComboBox::from_id_salt("path")
                    .selected_text(&p.path)
                    .show_ui(ui, |ui| {
                        for path in paths {
                            ui.selectable_value(&mut p.path, path.clone(), path);
                        }
                    });
                xy(ui, "Preset at", &mut p.at);
                if ui.small_button("Remove preset").clicked() {
                    remove = Some(i);
                }
            });
        });
    }
    if let Some(i) = remove {
        l.preset.remove(i);
    }
}
fn tile_form(
    ui: &mut egui::Ui,
    doc: &mut Document,
    brush: &Brush,
    pixel: &mut u8,
    message: &mut String,
) -> bool {
    let Some(mut tile) = doc
        .project
        .tiles
        .get(&brush.path)
        .and_then(|d| d.tiles.get(brush.index))
        .cloned()
    else {
        return false;
    };
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(&brush.path);
        ui.label("Rarity");
        changed |= ui.add(egui::DragValue::new(&mut tile.rarity)).changed();
        ui.label("Roof height");
        changed |= ui
            .add(egui::DragValue::new(&mut tile.roof_height))
            .changed();
    });
    ui.label("Collision flags per subtile (bit 1 blocks walking)");
    egui::Grid::new("map-collision").show(ui, |ui| {
        for y in 0..5 {
            for x in 0..5 {
                changed |= ui
                    .add(egui::DragValue::new(&mut tile.subtile_flags[y * 5 + x]).range(0..=255))
                    .changed();
            }
            ui.end_row();
        }
    });
    ui.horizontal(|ui| {
        ui.label("Palette index");
        ui.add(egui::DragValue::new(pixel).range(0..=255));
        ui.weak("0 is transparent; drag over a block to paint");
    });
    egui::ScrollArea::horizontal()
        .max_height(160.0)
        .id_salt("map-tile-blocks")
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                for (i, b) in tile.blocks.iter_mut().enumerate() {
                    let (w, h) = b.size();
                    let image = egui::ColorImage::from_rgba_unmultiplied([w, h], &grey(&b.pixels));
                    let texture = ui.ctx().load_texture(
                        format!("tile-block-{i}"),
                        image,
                        egui::TextureOptions::NEAREST,
                    );
                    let response = ui.add(
                        egui::Image::new((
                            texture.id(),
                            egui::vec2(w as f32 * 3.0, h as f32 * 3.0),
                        ))
                        .sense(egui::Sense::click_and_drag()),
                    );
                    if response.hovered() && ui.input(|s| s.pointer.primary_down()) {
                        if let Some(p) = response.interact_pointer_pos() {
                            let x = ((p.x - response.rect.left()) / 3.0) as usize;
                            let y = ((p.y - response.rect.top()) / 3.0) as usize;
                            if x < w && y < h {
                                let valid = if b.format == d2_formats::dt1::ISO_FORMAT {
                                    let half = if y < 8 { 2 * (y + 1) } else { 2 * (15 - y) };
                                    x >= 16 - half && x < 16 + half
                                } else {
                                    true
                                };
                                if valid && b.pixels[y * w + x] != *pixel {
                                    b.pixels[y * w + x] = *pixel;
                                    changed = true;
                                }
                            }
                        }
                    }
                }
            });
        });
    if changed {
        let mut dt1 = doc.project.tiles[&brush.path].clone();
        dt1.tiles[brush.index] = tile;
        if let Err(e) = doc.update_tiles(&brush.path, dt1) {
            *message = e.to_string();
            return false;
        }
    }
    changed
}
fn overlay(ui: &egui::Ui, l: &Level, rect: egui::Rect, pan: [i32; 2], scale: f32) {
    let point = |[x, y]: [u16; 2]| {
        let (x, y) = map::layout::cell_origin(i32::from(x), i32::from(y));
        egui::pos2(
            rect.left() + (x + 80 - pan[0]) as f32 * scale,
            rect.top() + (y + 40 - pan[1]) as f32 * scale,
        )
    };
    let painter = ui.painter().with_clip_rect(rect);
    let label = |at: [u16; 2], text: String, color: Color32| {
        let p = point(at);
        painter.circle_filled(p, 4.0, color);
        painter.text(
            p + egui::vec2(6.0, -6.0),
            egui::Align2::LEFT_BOTTOM,
            text,
            egui::FontId::proportional(12.0),
            color,
        );
    };
    label(l.entry, "Entry".into(), Color32::LIGHT_GREEN);
    for w in &l.warp {
        label(
            w.at,
            format!("Warp {} → {}", w.slot, w.to),
            Color32::LIGHT_BLUE,
        );
    }
    for o in &l.object {
        label(o.at, format!("Object {}", o.class), Color32::YELLOW);
    }
    for p in &l.preset {
        label(p.at, format!("Preset {}", p.path), Color32::LIGHT_GRAY);
    }
    for s in &l.spawn {
        let [x, y, w, h] = s.rect;
        let points = vec![
            point([x, y]),
            point([x.saturating_add(w), y]),
            point([x.saturating_add(w), y.saturating_add(h)]),
            point([x, y.saturating_add(h)]),
            point([x, y]),
        ];
        painter.add(egui::Shape::line(
            points,
            egui::Stroke::new(1.5_f32, Color32::LIGHT_RED),
        ));
        label(
            [x, y],
            format!("{} × {}", s.monster, s.count),
            Color32::LIGHT_RED,
        );
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn brushes_preserve_library_and_variant_when_tile_keys_repeat() {
        let root = std::env::temp_dir().join(format!("studio-map-brushes-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let mut doc = Document::starter(&root).unwrap();
        let mut other = doc.project.tiles["modkit/generated.dt1"].clone();
        other.tiles.insert(1, other.tiles[0].clone());
        doc.project.tiles.insert("other.dt1".into(), other);
        doc.project.maps.level[0].tiles = vec!["other.dt1".into()];
        let choices = brushes(&doc.project, &doc.project.maps.level[0], false);
        assert!(choices.iter().all(|b| b.path == "other.dt1"));
        assert_eq!(choices[0].key, choices[1].key);
        assert_ne!(choices[0].index, choices[1].index);
        assert_ne!(choices[0].label(), choices[1].label());
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn editor_draws_generated_maps_and_rolls_dozen_layouts_without_a_window() {
        let root = std::env::temp_dir().join(format!("studio-map-ui-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let _ = std::fs::remove_file(root.join("files/levels.toml"));
        let mut doc = Document::starter(&root).unwrap();
        let mut l = doc.project.maps.level[0].clone();
        l.generation = Generation::Maze {
            cell: 3,
            floor: 0x100001,
            wall: 0x200001,
            orientation: 1,
        };
        doc.update_level(l).unwrap();
        let previews = thumbnails(&doc.project, 1, 42, 32).unwrap();
        assert_eq!(previews.len(), 32);
        assert!(previews
            .iter()
            .all(|p| p.w <= 300 && p.h <= 180 && p.rgba.len() == p.w * p.h * 4));
        let layouts: std::collections::BTreeSet<_> =
            previews.iter().map(|p| p.rgba.clone()).collect();
        assert!(layouts.len() > 24);
        let mut editor = MapEditor {
            doc: Some(doc),
            zoom: -2,
            pan: [-2560, 0],
            previews,
            ..Default::default()
        };
        let ctx = egui::Context::default();
        for frame in 0..4 {
            if frame == 3 {
                let (tx, rx) = mpsc::channel();
                editor.batch = Some((editor.revision, rx));
                tx.send(Ok(vec![Thumbnail {
                    level: 1,
                    seed: 99,
                    w: 1,
                    h: 1,
                    rgba: vec![0; 4],
                    texture: None,
                }]))
                .unwrap();
                // Invalid in-progress forms must draw their validation error without panicking.
                editor.doc.as_mut().unwrap().project.maps.level[0]
                    .spawn
                    .push(Spawn {
                        monster: 0,
                        rect: [u16::MAX; 4],
                        count: 1,
                    });
                editor.revision += 1;
            }
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1200.0, 900.0),
                    )),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        editor.draw(ui, &root, Path::new(""), Path::new(""), false);
                    });
                },
            );
        }
        assert_eq!(editor.count, 32);
        assert!(editor.texture.is_some());
        assert!(editor.dirty());
        assert!(!editor.validation.is_empty());
        assert!(
            editor.previews.is_empty(),
            "Edits invalidate old seed thumbnails"
        );
        assert!(editor.batch.is_none(), "Stale worker results are discarded");
        std::fs::remove_dir_all(root).unwrap();
    }
}

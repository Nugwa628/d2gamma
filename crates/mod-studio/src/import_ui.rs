//! Import-time conversion jobs. No conversion is installed in the game renderer.
use eframe::egui::{self, Color32};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
pub struct Importer {
    source: String,
    destination: String,
    palette: String,
    sound: bool,
    library: bool,
    icon: bool,
    replace: bool,
    frame_width: u32,
    frame_height: u32,
    offset_x: i32,
    offset_y: i32,
    worker: Option<Receiver<Result<Imported, String>>>,
    status: String,
    preview: Option<Imported>,
    texture: Option<egui::TextureHandle>,
}
impl Default for Importer {
    fn default() -> Self {
        Self {
            source: String::new(),
            destination: String::new(),
            palette: String::new(),
            sound: false,
            library: false,
            icon: false,
            replace: false,
            frame_width: 32,
            frame_height: 32,
            offset_x: 0,
            offset_y: 0,
            worker: None,
            status: String::new(),
            preview: None,
            texture: None,
        }
    }
}
struct Imported {
    summary: String,
    image: Option<egui::ColorImage>,
    wave: Vec<f32>,
}
impl Importer {
    pub fn busy(&self) -> bool {
        self.worker.is_some()
    }
    pub fn poll(&mut self, ctx: &egui::Context) {
        if let Some(worker) = &self.worker {
            match worker.try_recv() {
                Ok(result) => {
                    self.worker = None;
                    match result {
                        Ok(p) => {
                            self.status = p.summary.clone();
                            self.texture = None;
                            self.preview = Some(p);
                        }
                        Err(e) => self.status = e,
                    }
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.worker = None;
                    self.status = "Import worker stopped".into();
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        if self.busy() {
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
        }
    }
    pub fn draw(&mut self, ui: &mut egui::Ui, root: &Path, native: &str) {
        self.poll(ui.ctx());
        ui.heading("Bake art and sound");
        ui.weak("Imports write native files into this project. Use your own art and sounds.");
        ui.horizontal(|ui| {
            if ui
                .selectable_label(!self.icon && !self.library && !self.sound, "PNG sprite")
                .clicked()
            {
                self.icon = false;
                self.library = false;
                self.sound = false;
            }
            if ui
                .selectable_label(!self.icon && !self.library && self.sound, "WAV sound")
                .clicked()
            {
                self.icon = false;
                self.library = false;
                self.sound = true;
            }
            if ui
                .selectable_label(!self.icon && self.library, "Native asset library")
                .clicked()
            {
                self.icon = false;
                self.library = true;
            }
            if ui
                .selectable_label(self.icon, "PNG icon / mod picture")
                .clicked()
            {
                self.icon = true;
            }
        });
        if self.icon {
            field(ui, "Source file", &mut self.source);
            field(ui, "PNG destination", &mut self.destination);
            ui.weak("icons/name.png for gameplay state icons; mod/picture.png for the Mods screen. PNG bytes are preserved.");
            ui.checkbox(&mut self.replace, "Replace existing destination files");
            if ui
                .add_enabled(!self.busy(), egui::Button::new("Validate and import PNG"))
                .clicked()
            {
                let (source, root, destination, replace) = (
                    PathBuf::from(&self.source),
                    root.to_owned(),
                    self.destination.clone(),
                    self.replace,
                );
                let (tx, rx) = mpsc::channel();
                self.worker = Some(rx);
                self.status = "Validating PNG...".into();
                std::thread::spawn(move || {
                    let result = crate::asset_library::icon(&root, &source, &destination, replace)
                        .map(|()| Imported {
                            summary: format!("Imported PNG {destination}"),
                            image: None,
                            wave: vec![],
                        })
                        .map_err(|e| format!("{e:#}"));
                    let _ = tx.send(result);
                });
            }
            ui.label(&self.status);
            return;
        }
        if self.library {
            ui.weak("Select an artist's data folder containing native sprites, COFs, DS1 maps, DT1 tile libraries or WAV sounds. Gameplay definitions are authored in their editors.");
            field(ui, "Source folder", &mut self.source);
            ui.checkbox(&mut self.replace, "Replace existing destination files");
            if ui
                .add_enabled(
                    !self.busy(),
                    egui::Button::new("Validate and import asset library"),
                )
                .clicked()
            {
                let (source, root, replace) =
                    (PathBuf::from(&self.source), root.to_owned(), self.replace);
                let (tx, rx) = mpsc::channel();
                self.worker = Some(rx);
                self.status = "Validating asset library...".into();
                std::thread::spawn(move || {
                    let result = crate::asset_library::import(&root, &source, replace)
                        .map(|n| Imported {
                            summary: format!("Imported {n} native asset files"),
                            image: None,
                            wave: vec![],
                        })
                        .map_err(|e| format!("{e:#}"));
                    let _ = tx.send(result);
                });
            }
            ui.label(&self.status);
            return;
        }
        field(ui, "Source file", &mut self.source);
        field(ui, "Engine destination", &mut self.destination);
        ui.weak(if self.sound {
            "Example: data/global/sfx/my-sound.wav · sounds FileName: my-sound.wav · output is 22050 Hz PCM16"
        } else {
            "Example: data/global/missiles/my-sprite.dc6 · frames across, directions down"
        });
        if !self.sound {
            if self.palette.is_empty() && !native.is_empty() {
                self.palette = Path::new(native)
                    .join("base/data/global/palette/act1/pal.dat.pal")
                    .display()
                    .to_string();
            }
            field(ui, "Palette .pal", &mut self.palette);
            ui.horizontal(|ui| {
                ui.label("Frame width / height");
                ui.add(egui::DragValue::new(&mut self.frame_width).range(1..=4096));
                ui.add(egui::DragValue::new(&mut self.frame_height).range(1..=4096));
            });
            ui.horizontal(|ui| {
                ui.label("Frame offsets X / Y");
                ui.add(egui::DragValue::new(&mut self.offset_x).range(-4096..=4096));
                ui.add(egui::DragValue::new(&mut self.offset_y).range(-4096..=4096));
            });
        }
        ui.checkbox(&mut self.replace, "Replace existing destination files");
        if ui
            .add_enabled(
                !self.busy(),
                egui::Button::new("Convert, validate and import"),
            )
            .clicked()
        {
            let (source, destination, palette, root) = (
                PathBuf::from(&self.source),
                self.destination.clone(),
                PathBuf::from(&self.palette),
                root.to_owned(),
            );
            let (sound, replace, frame, offset) = (
                self.sound,
                self.replace,
                (self.frame_width, self.frame_height),
                (self.offset_x, self.offset_y),
            );
            let (tx, rx) = mpsc::channel();
            self.worker = Some(rx);
            self.status = "Converting…".into();
            std::thread::spawn(move || {
                let result = (|| -> anyhow::Result<Imported> {
                    if std::fs::metadata(&source)?.len() > 64 * 1024 * 1024 {
                        anyhow::bail!("Source exceeds 64 MiB");
                    }
                    let bytes = std::fs::read(source)?;
                    if sound {
                        let (file, w) = crate::import::sound(&bytes, &destination)?;
                        crate::import::write(&root, &[file], replace)?;
                        let step = (w.samples.len() / 600).max(1);
                        let wave = w
                            .samples
                            .iter()
                            .step_by(step)
                            .map(|s| f32::from(*s) / 32768.)
                            .collect();
                        Ok(Imported {
                            summary: format!(
                                "Imported {} sample frames, {} channels, 22050 Hz PCM16",
                                w.samples.len() / usize::from(w.channels),
                                w.channels
                            ),
                            image: None,
                            wave,
                        })
                    } else {
                        let palette = crate::import::palette(std::fs::read(palette)?)?;
                        let imported =
                            crate::import::sprite(&bytes, &destination, frame, offset, &palette)?;
                        let f = imported
                            .sprite
                            .frames
                            .first()
                            .ok_or_else(|| anyhow::anyhow!("No frames"))?;
                        let image = crate::sprite_preview::color(
                            &crate::sprite_preview::Frame {
                                width: f.width,
                                height: f.height,
                                x: f.offset_x,
                                y: f.offset_y,
                                pixels: f.pixels.clone(),
                            },
                            &palette,
                        );
                        crate::import::write(&root, &imported.files, replace)?;
                        Ok(Imported {
                            summary: format!(
                                "Imported {} directions × {} frames; indexed PNG + DC6 sidecar",
                                imported.sprite.header.directions,
                                imported.sprite.header.frames_per_direction
                            ),
                            image: Some(image),
                            wave: Vec::new(),
                        })
                    }
                })()
                .map_err(|e| format!("{e:#}"));
                let _ = tx.send(result);
            });
        }
        if self.busy() {
            ui.spinner();
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(50));
        }
        ui.label(&self.status);
        if let Some(p) = &self.preview {
            if let Some(image) = &p.image {
                let t = self.texture.get_or_insert_with(|| {
                    ui.ctx().load_texture(
                        "import-preview",
                        image.clone(),
                        egui::TextureOptions::NEAREST,
                    )
                });
                ui.add(egui::Image::new(&*t).max_size(egui::vec2(600., 400.)));
            }
            if !p.wave.is_empty() {
                let (r, _) = ui.allocate_exact_size(
                    egui::vec2(ui.available_width().min(650.), 180.),
                    egui::Sense::hover(),
                );
                let painter = ui.painter_at(r);
                painter.rect_filled(r, 0., Color32::from_gray(20));
                let n = p.wave.len();
                let points = p
                    .wave
                    .iter()
                    .enumerate()
                    .map(|(i, v)| {
                        egui::pos2(
                            r.left() + i as f32 / (n - 1).max(1) as f32 * r.width(),
                            r.center().y - v * r.height() * 0.45,
                        )
                    })
                    .collect();
                painter.add(egui::Shape::line(
                    points,
                    egui::Stroke::new(1_f32, Color32::LIGHT_GREEN),
                ));
                ui.weak("Baked PCM waveform");
            }
        }
    }
}
fn field(ui: &mut egui::Ui, label: &str, text: &mut String) {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.add(egui::TextEdit::singleline(text).desired_width(500.));
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn import_completion_is_polled_even_when_pane_is_hidden() {
        let (tx, rx) = mpsc::channel();
        let mut i = Importer {
            worker: Some(rx),
            ..Default::default()
        };
        assert!(i.busy());
        assert!(tx
            .send(Ok(Imported {
                summary: "Imported".into(),
                image: None,
                wave: Vec::new(),
            }))
            .is_ok());
        i.poll(&egui::Context::default());
        assert!(!i.busy());
        assert_eq!(i.status, "Imported");
        assert_eq!((i.frame_width, i.frame_height), (32, 32));
    }
}

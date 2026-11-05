//! Mod panels rendered through the existing UI pass, so window and 4K
//! presentation use the same scaler as the built-in panels.

use super::{draw::CelCall, panels::UiFiles, text::TextOpts, *};
use d2_modkit::ui::{Kind, Panel as Definition};

pub fn load(source: &dyn crate::assets::path::FileSource) -> Result<Vec<Definition>, String> {
    let mut names = std::collections::BTreeSet::new();
    let mut replacements = std::collections::BTreeSet::new();
    source
        .ui_documents()?
        .into_iter()
        .map(|(path, bytes)| {
            let mut p =
                Definition::parse(std::str::from_utf8(&bytes).map_err(|e| format!("{path}: {e}"))?)
                    .map_err(|e| format!("{path}: {e}"))?;
            if !names.insert(p.name.clone()) {
                return Err(format!("{path}: duplicate panel name {}", p.name));
            }
            if let Some(r) = &p.replace {
                if builtin(r).is_none() || !replacements.insert(r.clone()) {
                    return Err(format!("{path}: unknown or duplicate replacement {r}"));
                }
            }
            super::data_bindings::resolve(source, &mut p).map_err(|e| format!("{path}: {e}"))?;
            Ok(p)
        })
        .collect()
}

fn builtin(name: &str) -> Option<PanelId> {
    match name {
        "hud" => Some(original::hud::HUD_PANEL),
        "border" => Some(original::BORDER_PANEL),
        "inventory" => Some(PanelId(1)),
        "character" => Some(PanelId(2)),
        "skilltree" => Some(PanelId(4)),
        "waypoint" => Some(PanelId(20)),
        "stash" => Some(PanelId(25)),
        "cube" => Some(PanelId(26)),
        "quest_log" => Some(PanelId(15)),
        "shop" => Some(PanelId(12)),
        "message_log" => Some(PanelId(24)),
        "escape_menu" => Some(original::esc_menu::ESC_PANEL),
        "npc_menu" => Some(original::npc_box::NPC_MENU_PANEL),
        "socket" => Some(original::npc_talk::SOCKET_PANEL),
        "dialog" => Some(original::npc_talk::DIALOG_PANEL),
        "gold_dialog" => Some(original::gold_dialog::GOLD_PANEL),
        "market" => Some(original::market_ui::MARKET_PANEL),
        "messages" => Some(original::game_messages::MESSAGES_PANEL),
        "overhead" => Some(original::overhead_ui::OVERHEAD_PANEL),
        "warnings" => Some(original::hud::WARN_PANEL),
        "top" => Some(original::TOP_PANEL),
        "hire" => Some(hire_list::HIRE_PANEL),
        _ => None,
    }
}

pub fn install(
    root: &mut UiRoot,
    files: &mut UiFiles,
    definitions: Vec<Definition>,
) -> Result<(), UiError> {
    for (i, def) in definitions.into_iter().enumerate() {
        for e in &def.elements {
            if e.kind == Kind::Image {
                files.add(&e.image);
            }
        }
        let id = def
            .replace
            .as_deref()
            .and_then(builtin)
            .unwrap_or(PanelId(0x400 + i as u16));
        let replace = def.replace.is_some();
        let panel = Box::new(DataPanel {
            id,
            def,
            files: files.clone(),
            pressed: None,
        });
        if replace {
            root.replace(panel)?;
        } else {
            root.add(panel)?;
            root.open(id)?;
        }
    }
    Ok(())
}

struct DataPanel {
    id: PanelId,
    def: Definition,
    files: UiFiles,
    pressed: Option<WidgetId>,
}

// UI roots draw in box space; the world-view pass later adds ui_left.
fn point(x: u16, y: u16) -> Point {
    point_in(x, y, ui_space::current().ui)
}

fn point_in(x: u16, y: u16, ui: crate::rules::camera::FrameSize) -> Point {
    Point::new(
        i32::from(x) * ui.width / 1280 - ui.ui_left(),
        i32::from(y) * ui.height / 720,
    )
}

fn element_rect(e: &d2_modkit::ui::Element, ui: crate::rules::camera::FrameSize) -> Rect {
    let a = point_in(e.x, e.y, ui);
    let b = point_in(e.x + e.width, e.y + e.height, ui);
    Rect::new(a.x, a.y, (b.x - a.x) as u16, (b.y - a.y) as u16)
}

impl Panel for DataPanel {
    fn id(&self) -> PanelId {
        self.id
    }
    fn draw_before(&self) -> Option<PanelId> {
        self.def.replace.is_none().then_some(original::TOP_PANEL)
    }
    fn rect(&self) -> Rect {
        if self.def.elements.iter().all(|e| e.action.is_none()) {
            return Rect::default();
        }
        let ui = ui_space::current().ui;
        Rect::new(-ui.ui_left(), 0, ui.width as u16, ui.height as u16)
    }
    fn hit(&self, p: Point) -> Option<WidgetId> {
        self.def
            .elements
            .iter()
            .enumerate()
            .rev()
            .find(|(_, e)| {
                e.action.is_some() && element_rect(e, ui_space::current().ui).contains(p)
            })
            .map(|(i, _)| WidgetId(i as u16))
    }
    fn covers_world_at(&self, p: Point) -> bool {
        self.hit(p).is_some()
    }
    fn ui_elements(&self, _: &UiCtx) -> Vec<ui_ids::UiElement> {
        self.def
            .elements
            .iter()
            .filter(|e| e.action.is_some())
            .map(|e| {
                ui_ids::UiElement::new(
                    format!("mod.button({},{})", self.def.name, e.id),
                    element_rect(e, ui_space::current().ui),
                )
            })
            .collect()
    }
    fn tick(&mut self, t: &PanelTick) -> UiResponse {
        if !t.open {
            self.pressed = None;
        }
        UiResponse::Ignored
    }
    fn event(&mut self, event: UiEvent, _: &UiCtx) -> UiResponse {
        match event {
            UiEvent::CursorLeft => {
                self.pressed = None;
                UiResponse::Ignored
            }
            UiEvent::Press {
                button: PointerButton::Left,
                at,
            } => {
                self.pressed = self.hit(at);
                if self.pressed.is_some() {
                    UiResponse::Consumed
                } else {
                    UiResponse::Ignored
                }
            }
            UiEvent::Release {
                button: PointerButton::Left,
                at,
            } => {
                let Some(pressed) = self.pressed.take() else {
                    return UiResponse::Ignored;
                };
                if self.hit(at) != Some(pressed) {
                    return UiResponse::Consumed;
                }
                let action = self.def.elements[usize::from(pressed.0)]
                    .action
                    .as_ref()
                    .expect("action button");
                let payload = if let Some(id) = action.strip_prefix("net.") {
                    id.parse::<u16>()
                        .ok()
                        .and_then(d2_proto::mod_ui::server_action)
                } else {
                    d2_proto::mod_ui::action(&format!("{}/{}", self.def.name, action))
                };
                let Some(payload) = payload else {
                    return UiResponse::Consumed;
                };
                match crate::bridge::intent::mod_message(&payload) {
                    Ok(bytes) => UiResponse::Intent(ClientIntent(bytes)),
                    Err(_) => UiResponse::Consumed,
                }
            }
            UiEvent::CursorMoved(at) if self.hit(at).is_some() => UiResponse::Consumed,
            _ => UiResponse::Ignored,
        }
    }
    fn draw(&self, ctx: &UiCtx, out: &mut dyn UiDrawSink) {
        let ui = ui_space::current().ui;
        let clip = Rect::new(-ui.ui_left(), 0, ui.width as u16, ui.height as u16);
        for e in &self.def.elements {
            let bound = e.binding.as_ref().map(|binding| {
                super::data_bindings::read(ctx.world, &self.def, &binding.source).and_then(
                    |value| {
                        let max = match &binding.maximum_source {
                            Some(s) => Some(super::data_bindings::read(ctx.world, &self.def, s)?),
                            None => None,
                        };
                        Some(binding.present(value, max, e.color))
                    },
                )
            });
            let (label, fill, color) = match bound {
                Some(Some(value)) => value,
                Some(None) => ("—".into(), 0, e.color),
                None => (e.text.clone(), e.value, e.color),
            };
            let a = point(e.x, e.y);
            let b = point(e.x + e.width, e.y + e.height);
            let (w, h) = (b.x - a.x, b.y - a.y);
            let rect = |x, y, w, h| UiDraw::Rect(RectRequest::sized(x, y, w, h, color, 0));
            match e.kind {
                Kind::Frame => {
                    out.push(rect(a.x, a.y, w, 1));
                    out.push(rect(a.x, b.y - 1, w, 1));
                    out.push(rect(a.x, a.y, 1, h));
                    out.push(rect(b.x - 1, a.y, 1, h));
                }
                Kind::Bar => out.push(rect(a.x, a.y, w * i32::from(fill) / 1000, h)),
                Kind::Button => out.push(rect(a.x, a.y, w, h)),
                Kind::Image => out.push(UiDraw::Image(ImageRequest {
                    image: ImageRef {
                        file: self.files.id(&e.image).expect("registered image"),
                        frame: e.frame,
                    },
                    at: Point::new(a.x, b.y - 1),
                    clip,
                    look: CelLook::PLAIN,
                    call: CelCall::Draw,
                })),
                Kind::Text => {}
            }
            if matches!(e.kind, Kind::Text | Kind::Button) {
                out.push(UiDraw::Text(TextRequest {
                    text: label.encode_utf16().collect(),
                    at: Point::new(a.x, b.y - 1),
                    style: TextStyle {
                        font: 0,
                        color: if e.binding.is_some() && color <= 12 {
                            color.into()
                        } else {
                            0
                        },
                    },
                    opts: TextOpts::default(),
                    clip,
                }));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bound_text_changes_only_with_server_model_values() {
        let def = Definition::parse("version=1\nname='counter'\n[[elements]]\nid='rage'\nkind='text'\n[elements.bind]\nsource='var.combat/rage'\nmaximum=100\nformat='percent'").unwrap();
        let panel = DataPanel {
            id: PanelId(0x400),
            def,
            files: UiFiles::new(&[]),
            pressed: None,
        };
        let mut world = crate::bridge::world::ClientWorld::default();
        for (value, expected) in [(None, "—"), (Some(42), "42%"), (Some(200), "100%")] {
            if let Some(value) = value {
                world.ui_variables.insert("combat/rage".into(), value);
            }
            let ctx = UiCtx {
                tick: 0,
                world: &world,
                strings: &NoStrings,
            };
            let mut draws = Vec::new();
            panel.draw(&ctx, &mut draws);
            let UiDraw::Text(text) = &draws[0] else {
                panic!("expected counter text")
            };
            assert_eq!(String::from_utf16(&text.text).unwrap(), expected);
        }
    }
    #[test]
    fn no_mod_keeps_the_draw_list_and_panels() {
        let mut root = UiRoot::new(Box::new(NoPanelRules));
        let original = original::OriginalUi::new(
            original::UiConfig {
                screen: layout::Screen::R800,
                expansion_installed: true,
            },
            None,
        )
        .unwrap();
        original.install(&mut root).unwrap();
        let mut files = UiFiles::new(&[]);
        let before = root.open_panels();
        let world = crate::bridge::world::ClientWorld::default();
        let ctx = UiCtx {
            tick: 0,
            world: &world,
            strings: &NoStrings,
        };
        let mut draws = Vec::new();
        root.draw(&ctx, &mut draws);
        install(&mut root, &mut files, Vec::new()).unwrap();
        let mut after = Vec::new();
        root.draw(&ctx, &mut after);
        assert_eq!(draws, after);
        assert_eq!(root.open_panels(), before);
        assert!(files.names().is_empty());
    }

    #[test]
    fn replacement_retains_slot_and_open_state() {
        let mut root = UiRoot::new(Box::new(NoPanelRules));
        let original = original::OriginalUi::new(
            original::UiConfig {
                screen: layout::Screen::R800,
                expansion_installed: true,
            },
            None,
        )
        .unwrap();
        original.install(&mut root).unwrap();
        let before = root.open_panels();
        let def = Definition::parse("version=1\nname='custom'\nreplace='hud'").unwrap();
        install(&mut root, &mut original.files(), vec![def]).unwrap();
        assert_eq!(before, root.open_panels());
        root.close(original::hud::HUD_PANEL).unwrap();
        assert!(!root.is_open(original::hud::HUD_PANEL).unwrap());
    }

    #[test]
    fn documented_replacements_exist_in_the_play_root() {
        let mut root = UiRoot::new(Box::new(NoPanelRules));
        let original = original::OriginalUi::new(
            original::UiConfig {
                screen: layout::Screen::R800,
                expansion_installed: true,
            },
            None,
        )
        .unwrap();
        original.install(&mut root).unwrap();
        let mut files = original.files();
        for target in [
            "hud",
            "border",
            "inventory",
            "character",
            "skilltree",
            "waypoint",
            "stash",
            "cube",
            "quest_log",
            "shop",
            "message_log",
            "escape_menu",
            "npc_menu",
            "socket",
            "dialog",
            "gold_dialog",
            "market",
            "messages",
            "overhead",
            "warnings",
            "top",
            "hire",
        ] {
            let def = Definition::parse(&format!("version=1\nname='custom'\nreplace='{target}'"))
                .unwrap();
            install(&mut root, &mut files, vec![def]).unwrap_or_else(|e| panic!("{target}: {e}"));
        }
    }
    #[test]
    fn buttons_send_only_the_declared_name_after_a_complete_click() {
        let def=Definition::parse("version=1\nname='hud'\n[[actions]]\nname='allocate'\nstat='strength'\ndelta=1\n[[elements]]\nid='add'\nkind='button'\nx=20\ny=30\nwidth=80\nheight=25\naction='allocate'").unwrap();
        let mut panel = DataPanel {
            id: PanelId(0x400),
            def,
            files: UiFiles::new(&[]),
            pressed: None,
        };
        let world = crate::bridge::world::ClientWorld::default();
        let ctx = UiCtx {
            tick: 0,
            world: &world,
            strings: &NoStrings,
        };
        let e = &panel.def.elements[0];
        let r = element_rect(e, ui_space::current().ui);
        let at = Point::new(r.x + 1, r.y + 1);
        assert_eq!(
            panel.event(
                UiEvent::Release {
                    button: PointerButton::Left,
                    at
                },
                &ctx
            ),
            UiResponse::Ignored
        );
        assert_eq!(
            panel.event(
                UiEvent::Press {
                    button: PointerButton::Left,
                    at
                },
                &ctx
            ),
            UiResponse::Consumed
        );
        let UiResponse::Intent(intent) = panel.event(
            UiEvent::Release {
                button: PointerButton::Left,
                at,
            },
            &ctx,
        ) else {
            panic!("click must send an intent");
        };
        let payload =
            d2_proto::envelope::payload(d2_proto::envelope::Direction::ToServer, &intent.0)
                .unwrap();
        assert_eq!(d2_proto::mod_ui::read_action(payload), Some("hud/allocate"));
        assert!(world.ui_variables.is_empty());
        panel.event(
            UiEvent::Press {
                button: PointerButton::Left,
                at,
            },
            &ctx,
        );
        assert_eq!(
            panel.event(
                UiEvent::Release {
                    button: PointerButton::Left,
                    at: Point::new(r.x - 1, r.y)
                },
                &ctx
            ),
            UiResponse::Consumed
        );
        panel.event(
            UiEvent::Press {
                button: PointerButton::Left,
                at,
            },
            &ctx,
        );
        panel.event(UiEvent::CursorLeft, &ctx);
        assert_eq!(
            panel.event(
                UiEvent::Release {
                    button: PointerButton::Left,
                    at
                },
                &ctx
            ),
            UiResponse::Ignored
        );
        assert!(!panel.covers_world_at(Point::new(r.x - 1, r.y)));
        assert!(panel.covers_world_at(at));
        assert!(ui_ids::valid_id(&panel.ui_elements(&ctx)[0].id));
        for (w, h, scale) in [(1280, 720, 1), (3840, 2160, 3)] {
            let ui = crate::rules::camera::FrameSize {
                width: w,
                height: h,
            };
            let r = element_rect(&panel.def.elements[0], ui);
            assert_eq!(
                (r.x + ui.ui_left(), r.y, i32::from(r.w), i32::from(r.h)),
                (20 * scale, 30 * scale, 80 * scale, 25 * scale)
            );
        }
        let mut plain = DataPanel {
            id: PanelId(0x401),
            def: Definition::parse("version=1\nname='plain'\n[[elements]]\nid='b'\nkind='button'")
                .unwrap(),
            files: UiFiles::new(&[]),
            pressed: None,
        };
        assert_eq!(plain.rect(), Rect::default());
        assert_eq!(
            plain.event(
                UiEvent::Press {
                    button: PointerButton::Left,
                    at
                },
                &ctx
            ),
            UiResponse::Ignored
        );
    }
    #[test]
    fn network_buttons_send_the_declared_id_in_the_existing_envelope() {
        let mut def=Definition::parse("version=1\nname='hud'\n[[elements]]\nid='add'\nkind='button'\nx=20\ny=30\nwidth=80\nheight=25\naction='net.allocate'").unwrap();
        let network = d2_modkit::net::Definition::parse(
            "version=1\n[[actions]]\nid=7\nname='allocate'\nstat='strength'\ndelta=1",
        )
        .unwrap();
        network.resolve_panel(&mut def).unwrap();
        let mut panel = DataPanel {
            id: PanelId(0x400),
            def,
            files: UiFiles::new(&[]),
            pressed: None,
        };
        let world = crate::bridge::world::ClientWorld::default();
        let ctx = UiCtx {
            tick: 0,
            world: &world,
            strings: &NoStrings,
        };
        let r = element_rect(&panel.def.elements[0], ui_space::current().ui);
        let at = Point::new(r.x + 1, r.y + 1);
        assert_eq!(
            panel.event(
                UiEvent::Press {
                    button: PointerButton::Left,
                    at
                },
                &ctx
            ),
            UiResponse::Consumed
        );
        let UiResponse::Intent(intent) = panel.event(
            UiEvent::Release {
                button: PointerButton::Left,
                at,
            },
            &ctx,
        ) else {
            panic!("click must send");
        };
        assert_eq!(intent.0[0], 0x71);
        let payload =
            d2_proto::envelope::payload(d2_proto::envelope::Direction::ToServer, &intent.0)
                .unwrap();
        assert_eq!(d2_proto::mod_ui::read_server_action(payload), Some(7));
        assert!(world.synced_values.is_empty());
        let mut bad = panel.def.clone();
        bad.elements[0].action = Some("net.missing".into());
        assert!(network.resolve_panel(&mut bad).is_err());
    }

    #[test]
    fn network_bindings_render_only_synced_server_values() {
        let def = Definition::parse("version=1\nname='counter'\n[[elements]]\nid='rage'\nkind='text'\n[elements.bind]\nsource='sync.rage'\nmaximum=100\nformat='percent'").unwrap();
        let panel = DataPanel {
            id: PanelId(0x400),
            def,
            files: UiFiles::new(&[]),
            pressed: None,
        };
        let mut world = crate::bridge::world::ClientWorld::default();
        for (value, expected) in [(None, "—"), (Some(42), "42%"), (Some(200), "100%")] {
            if let Some(value) = value {
                world.synced_values.insert("rage".into(), value);
            }
            let ctx = UiCtx {
                tick: 0,
                world: &world,
                strings: &NoStrings,
            };
            let mut draws = Vec::new();
            panel.draw(&ctx, &mut draws);
            let UiDraw::Text(text) = &draws[0] else {
                panic!("expected counter text")
            };
            assert_eq!(String::from_utf16(&text.text).unwrap(), expected);
        }
    }
}

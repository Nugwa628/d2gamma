//! The trademark screen: image (descriptor 2), 9 s timer
//! (3), logo halves (6, 7) and the legal text (4). Click, a key of the C2
//! list, or the timer → main menu.
//!
//! A mod's `intro.toml` ([`crate::mod_look::Intro`]) can change the timer,
//! leave the logo halves out and put its own lines in place of the legal
//! text; the default intro builds exactly the screen above.

use crate::mod_look::Intro;
use crate::ui::front_end::control::{Action, Control, ControlKind, TextRow};
use crate::ui::front_end::flow::Trigger;
use crate::ui::front_end::screen::{FrontCtx, Screen};
use crate::ui::front_end::screens::main_menu::{LOGO_LEFT, LOGO_RIGHT};
use crate::ui::front_end::{Registry, TRADEMARK};

/// Timer seconds (descriptor 3).
pub const TIMER_SECONDS: u32 = 9;

/// Keys whose key-down goes to the main menu (C2 r1): Backspace, Tab,
/// Enter, Space, Page Up/Down, End, Home, arrows, Delete, H, N, R, F1, Esc.
pub const KEYS: [u16; 18] = [
    8,
    9,
    13,
    32,
    33,
    34,
    35,
    36,
    37,
    38,
    39,
    40,
    46,
    b'H' as u16,
    b'N' as u16,
    b'R' as u16,
    112,
    27,
];

pub const BACKGROUND: &str = "FrontEnd\\trademarkscreenEXP";
pub const BACKGROUND_CLASSIC: &str = "FrontEnd\\trademark";

/// Text: descriptor 4, FontFormal12, (100, 580) 600×80, centred. The
/// builder adds the copyright string (expansion 21882, classic
/// 5104; its year "2001" / "2000" is replaced by "2000 - 2016") and string
/// 5105 "All Rights Reserved." (both rows
/// drawn, glyph for glyph).
const TEXT_FONT: u16 = 5;
/// Text colour `k` of both rows: 4 (the one that matches the shipped
/// screen; colours 0-12 tried).
pub const TEXT_COLOR: i32 = 4;
pub const STR_COPYRIGHT_EXP: u32 = 21882;
pub const STR_COPYRIGHT: u32 = 5104;
pub const STR_RESERVED: u32 = 5105;

struct Trademark {
    intro: Intro,
}

impl Screen for Trademark {
    fn build(&mut self, ctx: &mut FrontCtx) -> Vec<Control> {
        let go = Action::Trigger(Trigger::Continue);
        let art = if ctx.expansion {
            BACKGROUND
        } else {
            BACKGROUND_CLASSIC
        };
        let (id, year) = if ctx.expansion {
            (STR_COPYRIGHT_EXP, "2001")
        } else {
            (STR_COPYRIGHT, "2000")
        };
        let mut text = Control::new(ControlKind::Text, 100, 580, 600, 80).with_font(TEXT_FONT, 2);
        match &self.intro.text {
            // The mod's own lines, as plain text rows.
            Some(lines) => {
                for (i, line) in lines.iter().enumerate() {
                    if i == 0 {
                        text = text.with_text(line.clone(), TEXT_COLOR);
                    } else {
                        text.more_rows.push(TextRow {
                            string_id: 0,
                            text: line.clone(),
                            color: TEXT_COLOR,
                        });
                    }
                }
            }
            None => {
                text = text.with_string(id).with_text(
                    format!("{year}{}2000 - 2016", crate::ui::front_end::glyphs::SUBST),
                    TEXT_COLOR,
                );
                text.more_rows.push(TextRow {
                    string_id: STR_RESERVED,
                    text: String::new(),
                    color: TEXT_COLOR,
                });
            }
        }
        let mut v = vec![
            Control::new(ControlKind::Image, 0, 599, 800, 600)
                .with_art(art)
                .with_keys(&KEYS)
                .with_action(go),
            Control::timer(self.intro.seconds, go),
        ];
        if self.intro.logo {
            // Logo halves, descriptors 6 / 7.
            v.push(Control::new(ControlKind::AnimImage, 400, 120, 181, 170).with_art(LOGO_LEFT));
            v.push(Control::new(ControlKind::AnimImage, 400, 120, 188, 177).with_art(LOGO_RIGHT));
        }
        v.push(text);
        v
    }
}

pub fn register(reg: &mut Registry) {
    register_with(reg, Intro::default());
}

/// The trademark screen as `intro` says.
pub fn register_with(reg: &mut Registry, intro: Intro) {
    reg.register(TRADEMARK, screen(intro));
}

/// The trademark screen as `intro` says, to register in a running front end.
pub fn screen(intro: Intro) -> Box<dyn Screen> {
    Box::new(Trademark { intro })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::front_end::screens::ids::MAIN_MENU;
    use crate::ui::front_end::FrontEnd;

    fn kinds(f: &FrontEnd, k: ControlKind) -> Vec<&Control> {
        f.controls().iter().filter(|c| c.kind == k).collect()
    }

    fn started(intro: Intro) -> FrontEnd {
        let skip = intro.skip;
        let mut reg = Registry::default();
        crate::ui::front_end::screens::register_all(&mut reg);
        register_with(&mut reg, intro);
        let mut f = FrontEnd::new(true, Box::new(false), reg).with_skipped_intro(skip);
        f.start(true);
        f
    }

    #[test]
    fn the_default_intro_is_the_screen_the_game_always_had() {
        let f = started(Intro::default());
        assert_eq!(f.current(), TRADEMARK);
        assert_eq!(f.controls().len(), 5);
        assert_eq!(kinds(&f, ControlKind::Timer)[0].seconds, 9);
        assert_eq!(kinds(&f, ControlKind::AnimImage).len(), 2);
        let text = kinds(&f, ControlKind::Text)[0];
        assert_eq!(text.string_id, STR_COPYRIGHT_EXP);
        assert_eq!(text.color, TEXT_COLOR);
        assert!(text.text.as_deref().unwrap().starts_with("2001"));
        assert_eq!(text.more_rows.len(), 1);
        assert_eq!(text.more_rows[0].string_id, STR_RESERVED);
        // The screen registered with no intro at all is the same one.
        let mut reg = Registry::default();
        crate::ui::front_end::screens::register_all(&mut reg);
        let mut plain = FrontEnd::new(true, Box::new(false), reg);
        plain.start(true);
        assert_eq!(plain.controls(), f.controls());
    }

    #[test]
    fn an_intro_sets_the_time_the_logo_and_the_text() {
        let f = started(Intro {
            skip: false,
            seconds: 4,
            logo: false,
            text: Some(vec!["Ember Age".into(), "by Ada".into()]),
        });
        assert_eq!(kinds(&f, ControlKind::Timer)[0].seconds, 4);
        assert!(kinds(&f, ControlKind::AnimImage).is_empty());
        let text = kinds(&f, ControlKind::Text)[0];
        assert_eq!(
            (text.string_id, text.text.as_deref()),
            (0, Some("Ember Age"))
        );
        assert_eq!(text.more_rows.len(), 1);
        assert_eq!(
            (text.more_rows[0].string_id, text.more_rows[0].text.as_str()),
            (0, "by Ada")
        );
        // The picture and the keys that end the screen stay.
        assert_eq!(kinds(&f, ControlKind::Image).len(), 1);
    }

    #[test]
    fn an_intro_with_no_text_shows_none() {
        let f = started(Intro {
            text: Some(Vec::new()),
            ..Intro::default()
        });
        let text = kinds(&f, ControlKind::Text)[0];
        assert!(text.text.is_none() && text.more_rows.is_empty());
    }

    #[test]
    fn a_skipped_intro_opens_the_main_menu() {
        let f = started(Intro {
            skip: true,
            ..Intro::default()
        });
        assert_eq!(f.current(), MAIN_MENU);
        assert!(kinds(&f, ControlKind::Timer).is_empty());
    }

    #[test]
    fn the_timer_of_a_shorter_intro_ends_it() {
        let mut f = started(Intro {
            seconds: 2,
            ..Intro::default()
        });
        // 25 ticks a second; the timer fires once more than `seconds` whole
        // seconds have passed.
        for _ in 0..50 {
            f.tick();
        }
        assert_eq!(f.current(), TRADEMARK);
        for _ in 0..30 {
            f.tick();
        }
        assert_eq!(f.current(), MAIN_MENU);
    }
}

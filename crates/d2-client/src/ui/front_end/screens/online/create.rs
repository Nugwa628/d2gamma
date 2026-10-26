//! Character creation in online mode: the create screen as it is (class,
//! name, hardcore, expansion) plus a Ladder box beside Hardcore (a
//! character of the realm's current season), with its own sink, so nothing
//! is written to the save folder. OK sends the new character to the server; the wait box
//! is up until it answers. Created: back to the character list with it
//! first. Refused (name taken anywhere on the server, no free slot, a name
//! rule): the reason in a message box over the create screen, which keeps
//! the class and name as they were.
//!
//! The name-taken check the screen does itself looks at the account's own
//! characters; the server checks every account.

use crate::ui::front_end::flow::Trigger;
use crate::ui::front_end::screens::create::{CreateScreen, NewCharacterSink};
use crate::ui::front_end::{Action, Control, ControlKind, DrawItem, FrontCtx, Registry, Screen};
use crate::ui::geom::Point;
use crate::ui::ui_ids::UiElement;

use super::link::{failure_text, Event, Request};
use super::widgets::*;
use super::{OnlineHandle, ONLINE_CREATE};

/// Custom action ids, above the create screen's own.
pub mod act {
    pub const BASE: u32 = 0x1_0000;
    pub const MESSAGE_OK: u32 = BASE + 1;
    pub const CANCEL: u32 = BASE + 2;
    pub const LADDER: u32 = BASE + 3;
}

/// The Ladder box and its label, on the Hardcore row right of it:
/// `(box x, label x)`; the row's y is the Hardcore box's.
pub const LADDER_X: (i32, i32) = (469, 489);

pub struct OnlineCreate {
    online: OnlineHandle,
    inner: CreateScreen,
    sink: NewCharacterSink,
    waiting: bool,
    message: Option<String>,
    /// The Ladder box: on, the new character plays the current season.
    ladder: bool,
    /// Where the wrapper's pop-ups start in the built list, and their parts.
    own: Own,
}

/// The wrapper's controls in the built list: they stay in it and show or
/// hide (a rebuild would start the create screen over).
#[derive(Clone, Debug, Default)]
struct Own {
    start: usize,
    message: Vec<usize>,
    message_text: usize,
    wait: Vec<usize>,
    /// The Ladder label and box.
    ladder: Vec<usize>,
}

impl OnlineCreate {
    pub fn new(online: OnlineHandle) -> Self {
        let sink = NewCharacterSink::default();
        let names = online.clone();
        let taken = Box::new(move |name: &str| {
            names
                .borrow()
                .state
                .chars
                .iter()
                .any(|c| c.name.eq_ignore_ascii_case(name))
        });
        OnlineCreate {
            online,
            inner: CreateScreen::new(sink.clone(), taken),
            sink,
            waiting: false,
            message: None,
            ladder: false,
            own: Own::default(),
        }
    }

    pub fn inner(&self) -> &CreateScreen {
        &self.inner
    }

    fn modal_up(&self) -> bool {
        self.waiting || self.message.is_some()
    }

    /// The Ladder box shows with the Hardcore box (a class picked) and
    /// takes no clicks under the create screen's own pop-ups.
    fn ladder_offered(&self) -> bool {
        self.inner
            .state()
            .is_some_and(|st| st.hardcore_visible() && !st.warning && !st.name_taken)
    }

    /// Whether the Ladder box is ticked.
    pub fn ladder(&self) -> bool {
        self.ladder
    }

    fn route(&mut self, t: Option<Trigger>) -> Option<Trigger> {
        if t != Some(Trigger::Ok) {
            return t;
        }
        let n = self.sink.borrow_mut().take()?;
        self.online.borrow_mut().send(Request::CharCreate {
            name: n.name,
            class: n.class.id(),
            hardcore: n.hardcore,
            expansion: n.expansion,
            ladder: self.ladder,
        });
        self.waiting = true;
        None
    }
}

impl Screen for OnlineCreate {
    fn canvas_changed(&mut self, canvas: crate::ui::front_end::canvas::FrontCanvas) {
        self.inner.canvas_changed(canvas);
    }

    fn build(&mut self, ctx: &mut FrontCtx) -> Vec<Control> {
        self.waiting = false;
        self.message = None;
        self.ladder = false;
        self.sink.borrow_mut().take();
        let mut v = self.inner.build(ctx);
        let start = v.len();
        let mut own = Own {
            start,
            ..Own::default()
        };
        // The Hardcore row: the label 21 px below the box, as Hardcore's.
        let y = if ctx.expansion { 560 } else { 540 };
        own.ladder = vec![v.len(), v.len() + 1];
        v.push(
            Control::new(ControlKind::Text, LADDER_X.1, y + 21, 100, 32)
                .with_text("LADDER", 4)
                .with_font(FONT16, 0),
        );
        v.push(
            Control::new(ControlKind::Button, LADDER_X.0, y, 15, 16)
                .with_art("FrontEnd\\clickbox")
                .with_action(Action::Custom(act::LADDER))
                .with_on_press(),
        );
        let popups = v.len();
        push_message(&mut v, CREATE_POPUP, "", act::MESSAGE_OK);
        own.message = (popups..v.len()).collect();
        own.message_text = popups + 3;
        let wait = v.len();
        push_wait(
            &mut v,
            CREATE_POPUP,
            "Creating the character...",
            act::CANCEL,
        );
        own.wait = (wait..v.len()).collect();
        for c in &mut v[popups..] {
            c.top = true;
        }
        self.own = own;
        v
    }

    fn action(&mut self, ctx: &mut FrontCtx, id: u32) -> Option<Trigger> {
        match id {
            act::MESSAGE_OK => {
                self.message = None;
                None
            }
            act::LADDER => {
                if self.ladder_offered() && !self.modal_up() {
                    self.ladder = !self.ladder;
                }
                None
            }
            act::CANCEL => {
                // A late CharCreated still lands in the list.
                self.waiting = false;
                None
            }
            _ if self.modal_up() => None,
            _ => {
                let t = self.inner.action(ctx, id);
                self.route(t)
            }
        }
    }

    fn tick(&mut self, ctx: &mut FrontCtx) -> Option<Trigger> {
        let t = self.inner.tick(ctx);
        let out = self.route(t);
        let events = self.online.borrow_mut().take_events();
        for ev in events {
            match ev {
                Event::CharCreated(_) if self.waiting => {
                    self.waiting = false;
                    return Some(Trigger::Ok);
                }
                Event::Error(f) if self.waiting => {
                    self.waiting = false;
                    self.message = Some(failure_text(&f));
                }
                Event::Disconnected(_) => return Some(Trigger::OnlineLost),
                _ => {}
            }
        }
        out
    }

    fn char(&mut self, ctx: &mut FrontCtx, unit: u16) {
        if !self.modal_up() {
            self.inner.char(ctx, unit);
        }
    }

    fn pointer(&mut self, ctx: &mut FrontCtx, p: Point) {
        self.inner.pointer(ctx, p);
    }

    fn mouse(&mut self, ctx: &mut FrontCtx, down: bool, p: Point) -> Option<Trigger> {
        if self.modal_up() {
            return None;
        }
        let t = self.inner.mouse(ctx, down, p);
        self.route(t)
    }

    fn key(&mut self, ctx: &mut FrontCtx, k: u16) -> Option<Trigger> {
        if self.modal_up() {
            return None;
        }
        let t = self.inner.key(ctx, k);
        self.route(t)
    }

    fn overlay(&mut self, now_ms: u64, adv: &dyn Fn(u16, &[u16]) -> i32) -> Vec<DrawItem> {
        self.inner.overlay(now_ms, adv)
    }

    fn sync(&mut self, controls: &mut [Control]) {
        let start = self.own.start.min(controls.len());
        self.inner.sync(&mut controls[..start]);
        let modal = self.modal_up();
        for c in &mut controls[..start] {
            // The create screen's own pop-ups keep their own rule.
            if !c.top {
                c.inert = modal;
            }
        }
        for (ix, on) in [
            (&self.own.message, self.message.is_some()),
            (&self.own.wait, self.waiting && self.message.is_none()),
        ] {
            for &i in ix {
                if let Some(c) = controls.get_mut(i) {
                    c.visible = on;
                    c.enabled = on;
                    if c.action == Action::None {
                        c.enabled = true;
                    }
                }
            }
        }
        let offered = self.ladder_offered();
        for &i in &self.own.ladder {
            if let Some(c) = controls.get_mut(i) {
                c.visible = offered;
                c.inert = modal;
                c.checked = self.ladder;
            }
        }
        if let (Some(m), Some(c)) = (&self.message, controls.get_mut(self.own.message_text)) {
            c.text = Some(m.clone());
        }
    }

    fn first_wins(&self) -> Option<std::ops::Range<usize>> {
        self.inner.first_wins()
    }

    fn ui_elements(&self) -> Vec<UiElement> {
        self.inner.ui_elements()
    }

    fn loads_sky_palette(&self) -> bool {
        self.inner.loads_sky_palette()
    }

    fn palette(&self) -> Option<[&'static str; 2]> {
        self.inner.palette()
    }
}

pub fn register(reg: &mut Registry, online: OnlineHandle) {
    reg.register(ONLINE_CREATE, Box::new(OnlineCreate::new(online)));
}

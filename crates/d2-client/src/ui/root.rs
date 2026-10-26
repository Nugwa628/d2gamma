//! `UiRoot`: owns the UI's panels in a fixed order, draws the open panels
//! bottom-most first, routes events top-most first, and is the only place
//! intents leave the UI.

use super::draw::UiDrawSink;
use super::geom::{Point, Rect};
use super::panel::{
    ClientIntent, Panel, PanelAction, PanelId, PanelTick, UiCtx, UiEvent, UiInput, UiResponse,
    WidgetId,
};
use super::ui_ids::UiElement;
use crate::bridge::link::ServerLink;
use crate::bridge::{Bridge, BridgeError};

/// The UI states that draw the inventory family.
pub const INVENTORY_FAMILY: [u8; 8] = [1, 0x0C, 0x0E, 0x17, 0x19, 0x1A, 0x1C, 0x1D];

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum UiError {
    #[error("panel {0:?} added twice")]
    DuplicatePanel(PanelId),
    #[error("unknown panel {0:?}")]
    UnknownPanel(PanelId),
    #[error("panel rules closed {closed:?} while opening it")]
    RuleClosedOpening { closed: PanelId },
}

/// Open/close/stack rules for [`UiRoot::open`]. The game's UI state rules
/// (the conflict gate can refuse, and the call has side effects) are
/// [`super::states::UiStates`]; a root driven by it
/// mirrors its flags with [`UiRoot::sync_states`] instead of calling
/// `open`.
pub trait PanelRules {
    /// Called before `opening` opens, with the open panels bottom to top;
    /// returns the panels to close first.
    fn on_open(&mut self, opening: PanelId, open: &[PanelId]) -> Vec<PanelId>;
}

/// The neutral rule set (tests, D2RUST's own panels): opening a panel closes
/// nothing.
pub struct NoPanelRules;

impl PanelRules for NoPanelRules {
    fn on_open(&mut self, _opening: PanelId, _open: &[PanelId]) -> Vec<PanelId> {
        Vec::new()
    }
}

/// The top-most open panel under a point and its widget there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UiHit {
    pub panel: PanelId,
    pub widget: Option<WidgetId>,
}

/// Where an event went.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Routed {
    /// A panel consumed it or answered with an intent (queued).
    Panel(PanelId),
    /// No open panel took it; the caller hands it to the world input.
    Unhandled,
}

struct Slot {
    panel: Box<dyn Panel>,
    open: bool,
}

pub struct UiRoot {
    /// Diagnostic counts; never consulted by routing or panel rules.
    pub coverage: d2_sim::debug::coverage::Counters,
    /// Bottom-most first.
    slots: Vec<Slot>,
    rules: Box<dyn PanelRules>,
    cursor: Option<Point>,
    outbox: Vec<ClientIntent>,
    /// A C→S 0x30 (chat end) was queued since the last
    /// [`Self::take_chat_end`]: every UI path that sends it also ends the
    /// NPC interaction on the client.
    chat_end: bool,
    /// The panel actions the root does not carry out itself, oldest first
    /// ([`Self::take_requests`]).
    requests: Vec<PanelAction>,
    /// Key-downs for [`Self::run_keys`], in order.
    keys: Vec<u32>,
}

impl UiRoot {
    pub fn new(rules: Box<dyn PanelRules>) -> Self {
        Self {
            coverage: Default::default(),
            slots: Vec::new(),
            rules,
            cursor: None,
            outbox: Vec::new(),
            chat_end: false,
            requests: Vec::new(),
            keys: Vec::new(),
        }
    }

    /// Adds a closed panel above every panel added before it. The order
    /// is fixed from then on.
    pub fn add(&mut self, panel: Box<dyn Panel>) -> Result<(), UiError> {
        let id = panel.id();
        if self.slots.iter().any(|s| s.panel.id() == id) {
            return Err(UiError::DuplicatePanel(id));
        }
        self.slots.push(Slot { panel, open: false });
        Ok(())
    }

    /// Puts `panel` in the place of the panel with the same id, keeping
    /// its draw and input order and its open state; returns the panel it
    /// replaced. (A mod game's replacement panel, `crate::mods`.)
    pub fn replace(&mut self, panel: Box<dyn Panel>) -> Result<Box<dyn Panel>, UiError> {
        let i = self.index(panel.id())?;
        Ok(std::mem::replace(&mut self.slots[i].panel, panel))
    }

    fn index(&self, id: PanelId) -> Result<usize, UiError> {
        self.slots
            .iter()
            .position(|s| s.panel.id() == id)
            .ok_or(UiError::UnknownPanel(id))
    }

    pub fn is_open(&self, id: PanelId) -> Result<bool, UiError> {
        Ok(self.slots[self.index(id)?].open)
    }

    /// Open panels, bottom to top.
    pub fn open_panels(&self) -> Vec<PanelId> {
        self.slots
            .iter()
            .filter(|s| s.open)
            .map(|s| s.panel.id())
            .collect()
    }

    /// Opens a panel after closing what the rules name. Opening an open
    /// panel does not consult the rules. Nothing changes on an error.
    pub fn open(&mut self, id: PanelId) -> Result<(), UiError> {
        let i = self.index(id)?;
        if self.slots[i].open {
            return Ok(());
        }
        let close = self.rules.on_open(id, &self.open_panels());
        let mut close_idx = Vec::with_capacity(close.len());
        for c in close {
            if c == id {
                return Err(UiError::RuleClosedOpening { closed: c });
            }
            close_idx.push(self.index(c)?);
        }
        for c in close_idx {
            self.slots[c].open = false;
        }
        self.slots[i].open = true;
        self.coverage.hit("panels_opened", || id.0 as u32);
        Ok(())
    }

    pub fn close(&mut self, id: PanelId) -> Result<(), UiError> {
        let i = self.index(id)?;
        self.slots[i].open = false;
        Ok(())
    }

    pub fn toggle(&mut self, id: PanelId) -> Result<(), UiError> {
        if self.is_open(id)? {
            self.close(id)
        } else {
            self.open(id)
        }
    }

    /// Mirrors the game's UI state flags: every panel
    /// whose id is a UI state id 0–37 is open exactly when its flag is set.
    /// Other panels keep their state.
    pub fn sync_states(&mut self, states: &super::states::UiStates) {
        for s in &mut self.slots {
            let was_open = s.open;
            let id = s.panel.id().0;
            if usize::from(id) < super::layout::UI_STATE_COUNT {
                s.open = states.is_open(id as u8);
                // The inventory is drawn while any panel of its family
                // ([`INVENTORY_FAMILY`]) is open.
                if id == 1 && INVENTORY_FAMILY.iter().any(|&u| states.is_open(u)) {
                    s.open = true;
                }
                if self.coverage.enabled && s.open && !was_open {
                    self.coverage.hit("panels_opened", || id as u32);
                }
            }
        }
    }

    /// Draws the open panels, bottom-most first.
    pub fn draw(&self, ctx: &UiCtx, out: &mut dyn UiDrawSink) {
        let open: Vec<&Slot> = self.slots.iter().filter(|s| s.open).collect();
        for s in open.iter().filter(|s| s.panel.draw_first()) {
            s.panel.draw(ctx, out);
        }
        let open: Vec<&Slot> = open.into_iter().filter(|s| !s.panel.draw_first()).collect();
        let moved = |s: &Slot| {
            s.panel
                .draw_before()
                .is_some_and(|t| open.iter().any(|o| o.panel.id() == t))
        };
        for s in &open {
            // `Panel::draw_before`: the panels drawn right before this one.
            for b in open
                .iter()
                .filter(|b| b.panel.draw_before() == Some(s.panel.id()))
            {
                b.panel.draw(ctx, out);
            }
            if !moved(s) {
                s.panel.draw(ctx, out);
            }
        }
    }

    /// The top-most open panel whose rect contains `p`.
    pub fn hit(&self, p: Point) -> Option<UiHit> {
        self.slots
            .iter()
            .rev()
            .filter(|s| s.open && s.panel.rect().contains(p))
            .map(|s| UiHit {
                panel: s.panel.id(),
                widget: s.panel.hit(p),
            })
            .next()
    }

    /// Whether an open panel that `covers` names has its rect over `p`,
    /// whatever lies above it.
    pub fn open_panel_at(&self, p: Point, covers: impl Fn(PanelId) -> bool) -> bool {
        self.slots
            .iter()
            .any(|s| s.open && covers(s.panel.id()) && s.panel.rect().contains(p))
    }

    /// The clickable elements of the open panels ([`Panel::ui_elements`]),
    /// top-most panel first. An element whose click point a higher open
    /// panel claims is left out: a widget of it is there (its `hit`), or
    /// one of its own elements is (a dialog that takes a press anywhere).
    /// Panels that only draw over the frame (their rect is the screen but
    /// nothing of theirs is hit there) do not hide what is below. Each id
    /// is listed once (the top-most wins).
    pub fn ui_elements(&self, ctx: &UiCtx) -> Vec<UiElement> {
        let mut out: Vec<UiElement> = Vec::new();
        let mut above: Vec<&dyn Panel> = Vec::new();
        let mut above_rects: Vec<Rect> = Vec::new();
        for s in self.slots.iter().rev().filter(|s| s.open) {
            let els = s.panel.ui_elements(ctx);
            for e in &els {
                let covered = above_rects.iter().any(|r| r.contains(e.at))
                    || above
                        .iter()
                        .any(|p| p.rect().contains(e.at) && p.hit(e.at).is_some());
                if !covered && !out.iter().any(|o| o.id == e.id) {
                    out.push(e.clone());
                }
            }
            above_rects.extend(els.iter().map(|e| e.rect));
            above.push(s.panel.as_ref());
        }
        out
    }

    /// What is under the last cursor position the root saw.
    pub fn hovered(&self) -> Option<UiHit> {
        self.cursor.and_then(|p| self.hit(p))
    }

    /// Routes one event top-most first. A pointer event is offered
    /// to each open panel whose rect contains its point; other events to
    /// every open panel. The first answer other than `Ignored` ends the
    /// walk; an intent is queued for [`Self::forward`]. A click inside an
    /// open UI-state panel's area is consumed by it: that is the panel's
    /// answer.
    pub fn dispatch(&mut self, e: UiEvent, ctx: &UiCtx) -> Routed {
        match e {
            UiEvent::CursorMoved(p) => self.cursor = Some(p),
            UiEvent::CursorLeft => self.cursor = None,
            _ => {}
        }
        if matches!(e, UiEvent::CursorMoved(_) | UiEvent::CursorLeft) {
            super::ui_frame::set_pointer(self.cursor);
        }
        let at = e.at();
        for i in (0..self.slots.len()).rev() {
            let s = &mut self.slots[i];
            if !s.open || at.is_some_and(|p| !s.panel.rect().contains(p)) {
                continue;
            }
            let r = s.panel.event(e, ctx);
            let id = s.panel.id();
            if self.answer(r) {
                return Routed::Panel(id);
            }
        }
        Routed::Unhandled
    }

    /// Carries out a panel's answer; false when it ignored the event.
    fn answer(&mut self, r: UiResponse) -> bool {
        match r {
            UiResponse::Ignored => return false,
            UiResponse::Consumed => {}
            UiResponse::Intent(i) => self.queue_intent(i),
            UiResponse::Actions(actions) => {
                for a in actions {
                    self.act(a);
                }
            }
        }
        true
    }

    /// One [`PanelAction`]: intents are queued, a panel that is not a UI
    /// state opens or closes here, the rest waits in
    /// [`Self::take_requests`] for the UI's owner (the game's UI state
    /// rules, the sound and the screen messages).
    fn act(&mut self, a: PanelAction) {
        let own = |id: PanelId| usize::from(id.0) >= super::layout::UI_STATE_COUNT;
        match a {
            PanelAction::Intent(i) => self.queue_intent(i),
            // An unknown panel id changes nothing.
            PanelAction::Open(id) if own(id) => {
                let _ = self.open(id);
            }
            PanelAction::Close(id) if own(id) => {
                let _ = self.close(id);
            }
            other => self.requests.push(other),
        }
    }

    /// The panel actions left for the UI's owner since the last call
    /// (UI state panels opened or closed, sounds, notes), oldest first.
    pub fn take_requests(&mut self) -> Vec<PanelAction> {
        std::mem::take(&mut self.requests)
    }

    /// Queues a key-down (a Windows virtual-key code) for
    /// [`Self::run_keys`].
    pub fn push_key(&mut self, vk: u32) {
        self.keys.push(vk);
    }

    /// Offers each queued key-down to the open panels top-most first
    /// ([`Panel::key_down`]); the first answer other than `Ignored` takes
    /// it.
    pub fn run_keys(&mut self, ctx: &UiCtx) {
        for vk in std::mem::take(&mut self.keys) {
            for i in (0..self.slots.len()).rev() {
                if !self.slots[i].open {
                    continue;
                }
                let r = self.slots[i].panel.key_down(vk, ctx);
                if self.answer(r) {
                    break;
                }
            }
        }
    }

    /// The per-frame tick of every panel, bottom-most first
    /// ([`Panel::tick`]); `online` is whether the game's link is a realm's
    /// ([`PanelTick::online`]).
    pub fn tick(&mut self, ctx: &UiCtx, tables: &crate::bridge::world::ClientTables, online: bool) {
        for i in 0..self.slots.len() {
            let t = PanelTick {
                ui: ctx,
                tables,
                open: self.slots[i].open,
                online,
            };
            let r = self.slots[i].panel.tick(&t);
            self.answer(r);
        }
    }

    /// Drains `input` and routes every event in order; returns the events
    /// no panel took, in order, for the world input.
    pub fn pump(&mut self, input: &mut dyn UiInput, ctx: &UiCtx) -> Vec<UiEvent> {
        let mut events = Vec::new();
        input.drain(&mut events);
        events
            .into_iter()
            .filter(|&e| self.dispatch(e, ctx) == Routed::Unhandled)
            .collect()
    }

    /// Queued intents, oldest first.
    pub fn intents(&self) -> &[ClientIntent] {
        &self.outbox
    }

    /// Queues an intent a panel handed out beside its [`UiResponse`] (the
    /// original panels' outputs, [`super::original`]); it leaves with the
    /// next [`Self::forward`], after the intents queued before it.
    pub fn queue_intent(&mut self, intent: ClientIntent) {
        self.chat_end |= intent.0.first() == Some(&0x30);
        self.outbox.push(intent);
    }

    /// Whether a C→S 0x30 was queued since the last call.
    pub fn take_chat_end(&mut self) -> bool {
        std::mem::take(&mut self.chat_end)
    }

    pub fn take_intents(&mut self) -> Vec<ClientIntent> {
        std::mem::take(&mut self.outbox)
    }

    /// Sends the queued intents in order through the bridge (only the
    /// root forwards). On an error the failed intent and those after it
    /// stay queued.
    pub fn forward<L: ServerLink>(&mut self, bridge: &mut Bridge<L>) -> Result<usize, BridgeError> {
        let mut sent = 0;
        let result = loop {
            let Some(i) = self.outbox.get(sent) else {
                break Ok(sent);
            };
            if let Err(e) = bridge.send_bytes(&i.0) {
                break Err(e);
            }
            sent += 1;
        };
        self.outbox.drain(..sent);
        result
    }
}

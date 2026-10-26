//! The lobby's Guild panel (plan `lobby-ui.md` L12; lobby contract
//! sections 3.3 and 5), the Friends panel's fourth tab.
//!
//! In a guild: its tag and name, the roster (rank and where each member
//! is) and a box under it, with the buttons the player's rank may use:
//!
//! - every member: WHISPER (the selected member) and LEAVE (pressed twice);
//! - officers too: KICK (a member, pressed twice), INVITE (the name in the
//!   box) and MOTD (the box's text; an empty box takes today's message to
//!   edit);
//! - the leader too: PROMOTE, DEMOTE, LEADER (pressed twice) and DISBAND,
//!   which needs the guild's tag typed in the box.
//!
//! The message of the day is the line under the panel. Without a guild:
//! the invites to the player (ACCEPT or DECLINE the selected one) and the
//! form that creates a guild (a tag of 2-4 letters or digits and a name).
//!
//! The realm decides: a refused request says why under the panel, and the
//! guild is fetched again after every change and every guild notice.

use std::collections::VecDeque;

use crate::ui::front_end::{Control, ControlKind, DrawItem};
use crate::ui::geom::{Point, Rect};
use crate::ui::ui_ids::UiElement;

use super::friends_panel::{
    address, chip_controls, chip_items, chip_row, shown, text, width, Chip, RealmClock, COL_CLASS,
    FIELD_X, ROW_BASE, ROW_H, TAN, UNDER,
};
use super::link::{
    failure_text, guild_notice, guild_rank, Asked, ErrorCode, Event, Failure, GuildBrief,
    GuildInfo, GuildMember, GuildNotice, GuildOp, GuildQuery, Request, Where,
};
use super::lobby::{act as lobby_act, PanelOut, RightPanel};
use super::widgets::{field_controls, fit, show_field, Field, FIELD_H, FIELD_W, FONT16, FONT8};
use super::widgets::{GOLD, GREEN, GREY, RED, WHITE};
use super::OnlineHandle;

/// The panel's actions: `GUILD + n`, inside the Friends panel's range.
pub mod act {
    use super::lobby_act::PANEL;

    pub const GUILD: u32 = PANEL + 40;
    /// The boxes: the text (in a guild), the new guild's tag and name.
    pub const TEXT: u32 = GUILD;
    pub const TAG: u32 = GUILD + 1;
    pub const NAME: u32 = GUILD + 2;
    pub const WHISPER: u32 = GUILD + 3;
    pub const KICK: u32 = GUILD + 4;
    pub const PROMOTE: u32 = GUILD + 5;
    pub const DEMOTE: u32 = GUILD + 6;
    pub const LEADER: u32 = GUILD + 7;
    pub const INVITE: u32 = GUILD + 8;
    pub const MOTD: u32 = GUILD + 9;
    pub const LEAVE: u32 = GUILD + 10;
    pub const DISBAND: u32 = GUILD + 11;
    pub const CREATE: u32 = GUILD + 12;
    pub const ACCEPT: u32 = GUILD + 13;
    pub const DECLINE: u32 = GUILD + 14;
    pub const SCROLL_UP: u32 = GUILD + 15;
    pub const SCROLL_DOWN: u32 = GUILD + 16;
}

/// The heading's baseline, inside the panel.
const HEAD_BASE: i32 = 36;
/// The roster: its first row's top and the rows shown.
const ROSTER_TOP: i32 = 40;
pub const ROSTER_ROWS: usize = 4;
/// The box under the roster (cel bottom) and the two button rows.
const TEXT_BOTTOM: i32 = 140;
const BUTTONS_1: i32 = 144;
const BUTTONS_2: i32 = 164;
/// The rank and place columns.
const COL_RANK: i32 = COL_CLASS;
const COL_WHERE: i32 = 204;
/// Without a guild: the invites (first row's top, rows), their buttons,
/// the create form's boxes (cel bottoms) and its button.
const INVITES_TOP: i32 = 40;
pub const INVITE_ROWS: usize = 2;
const ANSWER_TOP: i32 = 80;
const TAG_BOTTOM: i32 = 128;
const NAME_BOTTOM: i32 = 156;
const CREATE_AT: (i32, i32, u16) = (218, 136, 90);
const RULES_BASE: i32 = 176;
/// The longest motd and names typed.
const TEXT_MAX: usize = 120;
const TAG_MAX: usize = 4;
const NAME_MAX: usize = 24;

/// A rank's name and colour.
pub fn rank_text(rank: u8) -> (&'static str, i32) {
    match rank {
        guild_rank::LEADER => ("Leader", GOLD),
        guild_rank::OFFICER => ("Officer", WHITE),
        _ => ("Member", GREY),
    }
}

/// The buttons a rank may use, as (label, action, name), in two rows.
pub fn buttons(rank: u8) -> [Vec<(&'static str, u32, &'static str)>; 2] {
    let officer = rank >= guild_rank::OFFICER && rank != guild_rank::NONE;
    let leader = rank == guild_rank::LEADER;
    let mut one = vec![("WHISPER", act::WHISPER, "whisper")];
    let mut two = Vec::new();
    if officer {
        one.push(("KICK", act::KICK, "kick"));
        two.push(("INVITE", act::INVITE, "invite"));
        two.push(("MOTD", act::MOTD, "motd"));
    }
    if leader {
        one.push(("PROMOTE", act::PROMOTE, "promote"));
        one.push(("DEMOTE", act::DEMOTE, "demote"));
        one.push(("LEADER", act::LEADER, "leader"));
    }
    two.push(("LEAVE", act::LEAVE, "leave"));
    if leader {
        two.push(("DISBAND", act::DISBAND, "disband"));
    }
    [one, two]
}

/// Where a member is (the own guild sees the place, not the game).
fn member_place(m: &GuildMember, now_ms: u64) -> (String, i32) {
    match m.place {
        Where::Offline if m.last_seen_ms == 0 => ("offline".into(), GREY),
        Where::Offline => (
            format!("seen {}", super::friends_panel::ago(now_ms, m.last_seen_ms)),
            GREY,
        ),
        Where::Lobby => ("in the lobby".into(), GREEN),
        Where::InGame => ("in a game".into(), GOLD),
        Where::Hidden => ("online".into(), GREEN),
    }
}

/// A refusal's line: the client's sentence for the code, and the realm's
/// own text after it when it says more ("Make another member leader
/// first.").
fn refusal(f: &Failure) -> String {
    let own = failure_text(f);
    if f.text.is_empty() || own.contains(f.text.as_str()) {
        own
    } else {
        format!("{own} {}", f.text)
    }
}

/// The boxes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Focus {
    Text,
    Tag,
    Name,
}

pub struct GuildPanel {
    pub online: OnlineHandle,
    /// The guild as the realm last said (`SocialState`).
    pub brief: Option<GuildBrief>,
    /// The guild and its members (`GuildGet Mine`), the invites.
    pub info: Option<GuildInfo>,
    pub members: Vec<GuildMember>,
    pub invites: Vec<GuildInfo>,
    /// The invites the realm counts (the tab's mark).
    invites_waiting: u16,
    /// The realm has no guilds.
    pub unsupported: bool,
    /// The selected member's [`address`], the selected invite's guild.
    pub selected: Option<String>,
    pub selected_invite: Option<u64>,
    pub first: usize,
    pub text: Field,
    pub tag: Field,
    pub name: Field,
    pub focus: Option<Focus>,
    /// A button that asks to be pressed again (LEAVE, KICK, LEADER), and
    /// for whom.
    armed: Option<(u32, String)>,
    /// The requests out, oldest first, with the line their success shows.
    asks: VecDeque<(Asked, String)>,
    stale: bool,
    pub note: Option<(String, i32)>,
    clock: RealmClock,
    now_ms: u64,
    /// The boxes' text controls in the built list.
    text_ix: Vec<(Focus, usize)>,
    pub(super) chips: Vec<Chip>,
    area: Rect,
}

impl GuildPanel {
    pub fn new(online: OnlineHandle) -> Self {
        GuildPanel {
            online,
            brief: None,
            info: None,
            members: Vec::new(),
            invites: Vec::new(),
            invites_waiting: 0,
            unsupported: false,
            selected: None,
            selected_invite: None,
            first: 0,
            text: Field::new(TEXT_MAX),
            tag: Field::new(TAG_MAX),
            name: Field::new(NAME_MAX),
            focus: None,
            armed: None,
            asks: VecDeque::new(),
            stale: false,
            note: None,
            clock: RealmClock::default(),
            now_ms: 0,
            text_ix: Vec::new(),
            chips: Vec::new(),
            area: super::lobby::PANEL,
        }
    }

    /// Invites waiting for an answer (none while in a guild).
    pub fn invites_waiting(&self) -> u16 {
        if self.in_guild() {
            0
        } else {
            self.invites_waiting
        }
    }

    /// The player is in a guild (as the realm last said).
    pub fn in_guild(&self) -> bool {
        self.brief.is_some()
            || self
                .info
                .as_ref()
                .is_some_and(|i| i.my_rank != guild_rank::NONE)
    }

    /// The player's rank ([`guild_rank::NONE`] without a guild).
    pub fn rank(&self) -> u8 {
        match (&self.brief, &self.info) {
            (Some(b), _) => b.my_rank,
            (None, Some(i)) => i.my_rank,
            _ => guild_rank::NONE,
        }
    }

    fn send(&self, r: Request) {
        self.online.borrow_mut().send(r);
    }

    fn fetch(&mut self) {
        self.stale = false;
        self.send(Request::GuildGet(GuildQuery::Mine));
        self.send(Request::GuildGet(GuildQuery::Invites));
    }

    /// A change goes out; `done` is what its success says.
    fn ask(&mut self, r: Request, done: String) -> Option<PanelOut> {
        if let Some(asked) = r.asked() {
            self.asks.push_back((asked, done));
        }
        self.armed = None;
        self.send(r);
        Some(PanelOut::Rebuild)
    }

    fn say(&mut self, text: impl Into<String>, colour: i32) -> Option<PanelOut> {
        self.note = Some((text.into(), colour));
        self.armed = None;
        Some(PanelOut::Rebuild)
    }

    /// The answer to the oldest of this panel's requests of that kind.
    fn answered(&mut self, asked: Asked, refused: Option<&Failure>) {
        let Some(i) = self.asks.iter().position(|(a, _)| *a == asked) else {
            return;
        };
        let (_, done) = self.asks.remove(i).unwrap_or((asked, String::new()));
        self.note = Some(match refused {
            Some(f) => (refusal(f), RED),
            None => (done, GREEN),
        });
        if refused.is_none() {
            self.stale = true;
            if matches!(
                asked,
                Asked::GuildManage | Asked::GuildInvite | Asked::GuildCreate
            ) {
                self.text.text.clear();
                self.tag.text.clear();
                self.name.text.clear();
                self.focus = None;
            }
        }
    }

    /// The selected member and the name requests use for them.
    fn member(&self) -> Option<&GuildMember> {
        let key = self.selected.as_ref()?;
        self.members.iter().find(|m| address(&m.who) == *key)
    }

    /// The player's own character name.
    fn me(&self) -> String {
        self.online
            .borrow()
            .state
            .selected
            .as_ref()
            .map(|c| c.name.clone())
            .unwrap_or_default()
    }

    /// The selected member for an action on someone else; a note when
    /// there is none.
    fn target(&mut self) -> Result<(String, String, u8), Option<PanelOut>> {
        let me = self.me();
        let Some(m) = self.member() else {
            return Err(self.say("Choose a member in the list first.", WHITE));
        };
        let (who, name, rank) = (address(&m.who), shown(&m.who).to_string(), m.rank);
        if !me.is_empty() && name.eq_ignore_ascii_case(&me) {
            return Err(self.say("Choose another member: that one is you.", WHITE));
        }
        Ok((who, name, rank))
    }

    /// True when `id` was pressed just before for the same `who`; else
    /// arms it and says `ask`.
    fn confirmed(&mut self, id: u32, who: &str, ask: String) -> bool {
        if self
            .armed
            .as_ref()
            .is_some_and(|(a, w)| *a == id && w == who)
        {
            return true;
        }
        self.note = Some((ask, GOLD));
        self.armed = Some((id, who.to_string()));
        false
    }

    fn guild_action(&mut self, id: u32) -> Option<PanelOut> {
        let tag = self
            .info
            .as_ref()
            .map(|i| i.tag.clone())
            .or_else(|| self.brief.as_ref().map(|b| b.tag.clone()))
            .unwrap_or_default();
        let typed = self.text.text.trim().to_string();
        match id {
            act::WHISPER => match self.target() {
                Ok((_, name, _)) => Some(PanelOut::Whisper(name)),
                Err(out) => out,
            },
            act::KICK => {
                let (who, name, _) = match self.target() {
                    Ok(t) => t,
                    Err(out) => return out,
                };
                if !self.confirmed(id, &who, format!("Press KICK again to kick {name}.")) {
                    return Some(PanelOut::Rebuild);
                }
                self.ask(
                    Request::GuildManage(GuildOp::Kick(who)),
                    format!("{name} was kicked."),
                )
            }
            act::PROMOTE | act::DEMOTE => {
                let (who, name, rank) = match self.target() {
                    Ok(t) => t,
                    Err(out) => return out,
                };
                if id == act::PROMOTE && rank != guild_rank::MEMBER {
                    return self.say(format!("{name} is already an officer."), WHITE);
                }
                if id == act::DEMOTE && rank != guild_rank::OFFICER {
                    return self.say(format!("{name} is not an officer."), WHITE);
                }
                let (op, done) = if id == act::PROMOTE {
                    (GuildOp::Promote(who), format!("{name} is an officer now."))
                } else {
                    (GuildOp::Demote(who), format!("{name} is a member now."))
                };
                self.ask(Request::GuildManage(op), done)
            }
            act::LEADER => {
                let (who, name, _) = match self.target() {
                    Ok(t) => t,
                    Err(out) => return out,
                };
                let ask = format!(
                    "Press LEADER again to make {name} the leader (you become an officer)."
                );
                if !self.confirmed(id, &who, ask) {
                    return Some(PanelOut::Rebuild);
                }
                self.ask(
                    Request::GuildManage(GuildOp::MakeLeader(who)),
                    format!("{name} leads the guild now."),
                )
            }
            act::INVITE if typed.is_empty() => {
                self.focus = Some(Focus::Text);
                self.say(
                    "Type the character to invite in the box, then press INVITE.",
                    WHITE,
                )
            }
            act::INVITE => self.ask(
                Request::GuildInvite { who: typed.clone() },
                format!("{} is invited to <{tag}>.", typed.trim_start_matches('*')),
            ),
            act::MOTD if typed.is_empty() => {
                // An empty box takes today's message to edit.
                let motd = self
                    .info
                    .as_ref()
                    .map(|i| i.motd.clone())
                    .unwrap_or_default();
                self.text.text = motd.chars().take(TEXT_MAX).collect();
                self.focus = Some(Focus::Text);
                self.say("Edit the message in the box, then press MOTD again.", WHITE)
            }
            act::MOTD => self.ask(
                Request::GuildManage(GuildOp::Motd(typed)),
                "The message of the day is saved.".into(),
            ),
            act::LEAVE => {
                if !self.confirmed(id, &tag, format!("Press LEAVE again to leave <{tag}>.")) {
                    return Some(PanelOut::Rebuild);
                }
                self.ask(
                    Request::GuildManage(GuildOp::Leave),
                    format!("You left <{tag}>."),
                )
            }
            act::DISBAND if !typed.eq_ignore_ascii_case(&tag) || tag.is_empty() => {
                self.focus = Some(Focus::Text);
                self.say(
                    format!("Type the guild's tag ({tag}) in the box, then press DISBAND."),
                    GOLD,
                )
            }
            act::DISBAND => self.ask(
                Request::GuildManage(GuildOp::Disband(typed)),
                format!("<{tag}> is disbanded."),
            ),
            _ => None,
        }
    }

    fn create(&mut self) -> Option<PanelOut> {
        let tag = self.tag.text.trim().to_string();
        let name = self.name.text.trim().to_string();
        if tag.len() < 2 {
            self.focus = Some(Focus::Tag);
            return self.say("Type a tag of 2-4 letters or digits.", WHITE);
        }
        if name.len() < 3 {
            self.focus = Some(Focus::Name);
            return self.say(
                "Type the guild's name (3-24 letters, digits and spaces).",
                WHITE,
            );
        }
        let done = format!("<{tag}> {name} is founded: you lead it.");
        self.ask(Request::GuildCreate { tag, name }, done)
    }

    fn answer_invite(&mut self, accept: bool) -> Option<PanelOut> {
        let inv = self
            .selected_invite
            .and_then(|g| self.invites.iter().find(|i| i.guild == g))
            .or(match self.invites.as_slice() {
                [one] => Some(one),
                _ => None,
            })
            .cloned();
        let Some(inv) = inv else {
            return self.say("Choose an invite first.", WHITE);
        };
        let done = if accept {
            format!("You joined <{}> {}.", inv.tag, inv.name)
        } else {
            format!("You declined <{}>.", inv.tag)
        };
        self.selected_invite = None;
        self.ask(
            Request::GuildAnswer {
                guild: inv.guild,
                accept,
            },
            done,
        )
    }

    fn notice(&mut self, n: &GuildNotice) {
        self.stale = true;
        let me = self.me();
        let mine = !me.is_empty() && n.who.eq_ignore_ascii_case(&me);
        let text = match n.what {
            guild_notice::INVITED_YOU => format!("{} invites you to <{}> {}.", n.by, n.tag, n.name),
            guild_notice::JOINED => format!("{} joined <{}>.", n.who, n.tag),
            guild_notice::LEFT => format!("{} left <{}>.", n.who, n.tag),
            guild_notice::KICKED if mine => format!("{} kicked you from <{}>.", n.by, n.tag),
            guild_notice::KICKED => format!("{} was kicked by {}.", n.who, n.by),
            guild_notice::PROMOTED => format!("{} is an officer now.", n.who),
            guild_notice::DEMOTED => format!("{} is a member now.", n.who),
            guild_notice::LEADER_CHANGED => format!("{} leads <{}> now.", n.who, n.tag),
            guild_notice::DISBANDED => format!("<{}> was disbanded.", n.tag),
            guild_notice::MOTD => "The message of the day changed.".into(),
            _ => return,
        };
        // A request's own answer says more; a notice fills an empty line.
        if self.asks.is_empty() && self.note.is_none() {
            self.note = Some((text, WHITE));
        }
    }

    fn scroll(&mut self, d: i32) {
        let max = self.members.len().saturating_sub(ROSTER_ROWS);
        self.first = (self.first as i32 + d).clamp(0, max as i32) as usize;
    }

    fn step(&mut self, d: i32) {
        let keys: Vec<String> = self.members.iter().map(|m| address(&m.who)).collect();
        if keys.is_empty() {
            return;
        }
        let at = self
            .selected
            .as_ref()
            .and_then(|k| keys.iter().position(|o| o == k))
            .map_or(0, |i| {
                (i as i32 + d).clamp(0, keys.len() as i32 - 1) as usize
            });
        self.selected = Some(keys[at].clone());
        self.armed = None;
        if at < self.first {
            self.first = at;
        } else if at >= self.first + ROSTER_ROWS {
            self.first = at + 1 - ROSTER_ROWS;
        }
    }

    fn row_top(&self, r: usize) -> i32 {
        self.area.y + ROSTER_TOP + ROW_H * r as i32
    }

    fn invite_top(&self, r: usize) -> i32 {
        self.area.y + INVITES_TOP + ROW_H * r as i32
    }

    /// A box: its label, art, click area and text control.
    fn field(&mut self, v: &mut Vec<Control>, label: &str, bottom: i32, id: u32, f: Focus) {
        let area = self.area;
        v.push(
            Control::new(
                ControlKind::Text,
                area.x + 4,
                bottom - 4,
                FIELD_X as u16 - 6,
                20,
            )
            .with_font(FONT8, 0x20)
            .with_text(label, GOLD),
        );
        v.extend(field_controls(area.x + FIELD_X, bottom, id));
        self.text_ix.push((f, v.len() - 1));
    }

    fn field_rect(&self, bottom: i32) -> Rect {
        Rect::new(
            self.area.x + FIELD_X,
            self.area.y + bottom - i32::from(FIELD_H),
            FIELD_W,
            FIELD_H,
        )
    }

    fn the_field(&mut self, f: Focus) -> &mut Field {
        match f {
            Focus::Text => &mut self.text,
            Focus::Tag => &mut self.tag,
            Focus::Name => &mut self.name,
        }
    }

    fn chips_now(&self, area: Rect) -> Vec<Chip> {
        let own =
            |items: Vec<(&'static str, u32, &'static str)>| -> Vec<(&'static str, u32, String)> {
                items
                    .into_iter()
                    .map(|(l, id, n)| (l, id, format!("guild.{n}")))
                    .collect()
            };
        if self.in_guild() {
            let [one, two] = buttons(self.rank());
            let mut chips = chip_row(area, BUTTONS_1, &own(one));
            chips.extend(chip_row(area, BUTTONS_2, &own(two)));
            for c in &mut chips {
                c.on = self.armed.as_ref().is_some_and(|(id, _)| *id == c.id);
            }
            chips
        } else {
            let mut chips = Vec::new();
            if !self.invites.is_empty() {
                chips = chip_row(
                    area,
                    ANSWER_TOP,
                    &own(vec![
                        ("ACCEPT", act::ACCEPT, "accept"),
                        ("DECLINE", act::DECLINE, "decline"),
                    ]),
                );
            }
            let (x, y, w) = CREATE_AT;
            chips.push(Chip {
                at: Rect::new(area.x + x, area.y + y, w, super::friends_panel::CHIP_H),
                label: "CREATE",
                id: act::CREATE,
                name: "guild.create".into(),
                on: false,
            });
            chips
        }
    }

    /// The lines under the panel: the note, then the motd.
    fn under_lines(&self) -> Vec<(String, i32)> {
        let mut lines = Vec::new();
        if let Some(n) = &self.note {
            lines.push(n.clone());
        }
        if let Some(i) = self.info.as_ref().filter(|_| self.in_guild()) {
            if !i.motd.is_empty() {
                lines.push((format!("Motd: {}", i.motd), TAN));
            }
        }
        lines.truncate(UNDER.len());
        lines
    }

    fn roster_overlay(&self, area: Rect, adv: &dyn Fn(u16, &[u16]) -> i32) -> Vec<DrawItem> {
        let w16 = width(adv, FONT16);
        let w8 = width(adv, FONT8);
        let now = self.clock.now(self.now_ms);
        let me = self.me();
        let mut out = Vec::new();
        let (tag, name, count) = match (&self.info, &self.brief) {
            (Some(i), _) => (i.tag.clone(), i.name.clone(), Some(i.members)),
            (None, Some(b)) => (b.tag.clone(), b.name.clone(), None),
            _ => (String::new(), String::new(), None),
        };
        let (rank, rank_colour) = rank_text(self.rank());
        let side = match count {
            Some(n) => format!("{n} member{} - you: {rank}", if n == 1 { "" } else { "s" }),
            None => format!("you: {rank}"),
        };
        let side_w = w8(&side);
        let head_room = i32::from(area.w) - 12 - side_w;
        let head = fit(&format!("<{tag}> {name}"), head_room, &w16);
        out.push(text(FONT16, area.x + 4, area.y + HEAD_BASE, GOLD, &head));
        let side_x = area.x + i32::from(area.w) - 4 - side_w;
        out.push(text(FONT8, side_x, area.y + HEAD_BASE, rank_colour, &side));
        let right = if self.members.len() > ROSTER_ROWS {
            area.x + super::friends_panel::ROWS_RIGHT
        } else {
            area.x + i32::from(area.w) - 4
        };
        if self.members.is_empty() {
            out.push(text(
                FONT16,
                area.x + 6,
                self.row_top(1) + ROW_BASE,
                GREY,
                "Getting the members...",
            ));
        }
        for (i, m) in self
            .members
            .iter()
            .enumerate()
            .skip(self.first)
            .take(ROSTER_ROWS)
        {
            let top = self.row_top(i - self.first);
            let base = top + ROW_BASE;
            if self.selected.as_deref() == Some(address(&m.who).as_str()) {
                out.push(DrawItem::Border {
                    at: Point::new(area.x, top),
                    w: right - area.x + 2,
                    h: ROW_H,
                });
            }
            let online = m.place != Where::Offline;
            let n = shown(&m.who);
            let colour = if !me.is_empty() && n.eq_ignore_ascii_case(&me) {
                GOLD
            } else if online {
                WHITE
            } else {
                GREY
            };
            let name = fit(n, COL_RANK - 8, &w16);
            out.push(text(FONT16, area.x + 4, base, colour, &name));
            let (r, rc) = rank_text(m.rank);
            out.push(text(FONT8, area.x + COL_RANK, base, rc, r));
            let (p, pc) = member_place(m, now);
            let room = right - (area.x + COL_WHERE);
            out.push(text(
                FONT8,
                area.x + COL_WHERE,
                base,
                pc,
                &fit(&p, room, &w8),
            ));
        }
        out
    }

    fn invites_overlay(&self, area: Rect, adv: &dyn Fn(u16, &[u16]) -> i32) -> Vec<DrawItem> {
        let w16 = width(adv, FONT16);
        let mut out = vec![text(
            FONT16,
            area.x + 4,
            area.y + HEAD_BASE,
            GOLD,
            if self.invites.is_empty() {
                "You are in no guild."
            } else {
                "Invites to a guild:"
            },
        )];
        if self.invites.is_empty() {
            out.push(text(
                FONT8,
                area.x + 6,
                self.invite_top(1),
                GREY,
                "No invites. Ask a guild's officer, or found your own below.",
            ));
        }
        for (i, g) in self.invites.iter().take(INVITE_ROWS).enumerate() {
            let top = self.invite_top(i);
            let one = self.invites.len() == 1;
            if self.selected_invite == Some(g.guild) || (one && self.selected_invite.is_none()) {
                out.push(DrawItem::Border {
                    at: Point::new(area.x, top),
                    w: i32::from(area.w) - 2,
                    h: ROW_H,
                });
            }
            let line = format!("<{}> {}", g.tag, g.name);
            out.push(text(
                FONT16,
                area.x + 4,
                top + ROW_BASE,
                WHITE,
                &fit(&line, COL_WHERE - 8, &w16),
            ));
            let n = format!(
                "{} member{}",
                g.members,
                if g.members == 1 { "" } else { "s" }
            );
            out.push(text(FONT8, area.x + COL_WHERE, top + ROW_BASE, GREY, &n));
        }
        out.push(text(
            FONT8,
            area.x + 4,
            area.y + RULES_BASE,
            GREY,
            "Tags are 2-4 letters or digits, shown as <TAG>.",
        ));
        out
    }
}

impl RightPanel for GuildPanel {
    fn open(&mut self) {
        self.note = None;
        self.armed = None;
        self.focus = None;
        if !self.unsupported {
            self.fetch();
        }
    }

    fn feed(&mut self, ev: &Event) {
        match ev {
            Event::SocialState(s) => {
                self.clock.set(s.now_ms, self.now_ms);
                let before = self.brief.as_ref().map(|b| (b.guild, b.my_rank));
                self.brief = s.guild.as_ref().map(|g| GuildBrief {
                    guild: g.guild,
                    tag: g.tag.clone(),
                    name: g.name.clone(),
                    my_rank: g.my_rank,
                });
                self.invites_waiting = s.guild_invites;
                let after = self.brief.as_ref().map(|b| (b.guild, b.my_rank));
                if before != after {
                    if after.is_none() {
                        self.info = None;
                        self.members.clear();
                        self.selected = None;
                    }
                    self.armed = None;
                    self.stale = true;
                }
            }
            Event::Guild { info, members } => {
                self.info = Some(info.clone());
                self.members = members.clone();
                self.unsupported = false;
                let max = self.members.len().saturating_sub(ROSTER_ROWS);
                self.first = self.first.min(max);
                if self.member().is_none() {
                    self.selected = None;
                }
            }
            Event::GuildInvites(list) => {
                self.invites = list.clone();
                self.invites_waiting = list.len() as u16;
                if !self
                    .selected_invite
                    .is_some_and(|g| list.iter().any(|i| i.guild == g))
                {
                    self.selected_invite = None;
                }
            }
            Event::GuildNotice(n) => self.notice(n),
            Event::Refused {
                asked: Asked::GuildGet,
                failure,
            } => match failure.code {
                ErrorCode::NOT_SUPPORTED => self.unsupported = true,
                ErrorCode::NOT_IN_GUILD => {
                    self.info = None;
                    self.members.clear();
                    self.selected = None;
                }
                _ => self.note = Some((failure_text(failure), RED)),
            },
            Event::Done(d) => self.answered(d.asked, None),
            Event::Refused { asked, failure } => self.answered(*asked, Some(failure)),
            _ => {}
        }
    }

    fn controls(&mut self, area: Rect, v: &mut Vec<Control>) {
        self.area = area;
        self.text_ix.clear();
        if self.stale && !self.unsupported {
            self.fetch();
        }
        if self.unsupported {
            self.chips.clear();
            return;
        }
        if self.in_guild() {
            if self.members.len() > ROSTER_ROWS {
                let top = area.y + ROSTER_TOP;
                let span = ROW_H * ROSTER_ROWS as i32;
                let range = (self.members.len() - ROSTER_ROWS) as i32;
                let cel = |frame: u32, top: i32, id: Option<u32>| {
                    let mut c = Control::new(ControlKind::Image, area.x + 298, top + 14, 12, 14)
                        .with_art(super::widgets::SCROLL);
                    c.cel = Some(frame);
                    if let Some(id) = id {
                        c.action = crate::ui::front_end::Action::Custom(id);
                    }
                    c
                };
                v.push(cel(0, top, Some(act::SCROLL_UP)));
                v.push(cel(1, top + span - 14, Some(act::SCROLL_DOWN)));
                let track = span - 14 * 3;
                v.push(cel(4, top + 14 + track * self.first as i32 / range, None));
            }
            self.field(v, "TEXT", area.y + TEXT_BOTTOM, act::TEXT, Focus::Text);
        } else {
            self.field(v, "TAG", area.y + TAG_BOTTOM, act::TAG, Focus::Tag);
            self.field(v, "NAME", area.y + NAME_BOTTOM, act::NAME, Focus::Name);
        }
        let chips = self.chips_now(area);
        chip_controls(&chips, v);
        self.chips = chips;
    }

    fn overlay(
        &mut self,
        area: Rect,
        adv: &dyn Fn(u16, &[u16]) -> i32,
        now_ms: u64,
    ) -> Vec<DrawItem> {
        self.now_ms = now_ms;
        if self.unsupported {
            return vec![text(
                FONT16,
                area.x + 6,
                area.y + 80,
                GREY,
                "This realm has no guilds yet.",
            )];
        }
        let mut out = chip_items(&self.chips, adv);
        if self.in_guild() {
            out.extend(self.roster_overlay(area, adv));
        } else {
            out.extend(self.invites_overlay(area, adv));
        }
        let w8 = width(adv, FONT8);
        let bottom = area.y + i32::from(area.h);
        for (dy, (t, c)) in UNDER.iter().zip(self.under_lines()) {
            let line = fit(&t, i32::from(area.w) - 8, &w8);
            out.push(text(FONT8, area.x + 4, bottom + dy, c, &line));
        }
        out
    }

    fn sync(&mut self, controls: &mut [Control], now_ms: u64) {
        self.now_ms = now_ms;
        for (f, i) in self.text_ix.clone() {
            let focused = self.focus == Some(f);
            let field = self.the_field(f).clone();
            if let Some(c) = controls.get_mut(i) {
                show_field(c, &field, focused, now_ms, 18);
            }
        }
    }

    fn action(&mut self, id: u32) -> Option<PanelOut> {
        if id != self.armed.as_ref().map_or(0, |(a, _)| *a) {
            self.armed = None;
        }
        match id {
            act::TEXT => {
                self.focus = Some(Focus::Text);
                None
            }
            act::TAG => {
                self.focus = Some(Focus::Tag);
                None
            }
            act::NAME => {
                self.focus = Some(Focus::Name);
                None
            }
            act::CREATE => self.create(),
            act::ACCEPT => self.answer_invite(true),
            act::DECLINE => self.answer_invite(false),
            act::SCROLL_UP => {
                self.scroll(-1);
                Some(PanelOut::Rebuild)
            }
            act::SCROLL_DOWN => {
                self.scroll(1);
                Some(PanelOut::Rebuild)
            }
            _ if self.in_guild() => {
                // Only the buttons the rank shows act.
                let [one, two] = buttons(self.rank());
                if one.iter().chain(&two).any(|(_, b, _)| *b == id) {
                    self.guild_action(id)
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// Enter in a box: INVITE (in a guild, for the ranks that may) or
    /// CREATE; the box keeps the key either way.
    fn enter(&mut self) -> Option<PanelOut> {
        match self.focus? {
            Focus::Text => self.action(act::INVITE).or(Some(PanelOut::Rebuild)),
            Focus::Tag | Focus::Name => self.create(),
        }
    }

    fn blur(&mut self) {
        self.focus = None;
    }

    fn mouse(&mut self, p: Point, _now_ms: u64) -> Option<PanelOut> {
        if self.chips.iter().any(|c| c.at.contains(p)) {
            return None;
        }
        let boxes: Vec<Rect> = if self.in_guild() {
            vec![self.field_rect(TEXT_BOTTOM)]
        } else {
            vec![self.field_rect(TAG_BOTTOM), self.field_rect(NAME_BOTTOM)]
        };
        if boxes.iter().any(|b| b.contains(p)) {
            return None;
        }
        self.focus = None;
        let area = self.area;
        if self.in_guild() {
            let top = self.row_top(0);
            let rows = ROW_H * ROSTER_ROWS as i32;
            if p.y < top
                || p.y >= top + rows
                || p.x >= area.x + super::friends_panel::ROWS_RIGHT + 2
            {
                return Some(PanelOut::Rebuild);
            }
            let i = self.first + ((p.y - top) / ROW_H) as usize;
            let key = address(&self.members.get(i)?.who);
            if self.selected.as_ref() != Some(&key) {
                self.armed = None;
            }
            self.selected = Some(key);
        } else {
            let top = self.invite_top(0);
            if p.y < top || p.y >= top + ROW_H * INVITE_ROWS as i32 {
                return Some(PanelOut::Rebuild);
            }
            let i = ((p.y - top) / ROW_H) as usize;
            self.selected_invite = Some(self.invites.get(i)?.guild);
        }
        Some(PanelOut::Rebuild)
    }

    fn wheel(&mut self, delta: i32) -> Option<PanelOut> {
        self.scroll(-delta.signum());
        Some(PanelOut::Rebuild)
    }

    fn char(&mut self, c: char) -> bool {
        let Some(f) = self.focus else {
            return false;
        };
        let ok: fn(char) -> bool = match f {
            Focus::Text => |c| !c.is_control(),
            Focus::Tag => |c| c.is_ascii_alphanumeric(),
            Focus::Name => |c| c.is_alphanumeric() || c == ' ',
        };
        self.the_field(f).type_char(c, ok);
        true
    }

    fn key(&mut self, k: u16) -> bool {
        use crate::ui::front_end::control::vk;
        const UP: u16 = 38;
        const DOWN: u16 = 40;
        const TAB: u16 = 9;
        match (k, self.focus) {
            (vk::BACKSPACE, Some(f)) => self.the_field(f).backspace(),
            (TAB, Some(Focus::Tag)) => self.focus = Some(Focus::Name),
            (TAB, Some(Focus::Name)) => self.focus = Some(Focus::Tag),
            (UP, None) if self.in_guild() => self.step(-1),
            (DOWN, None) if self.in_guild() => self.step(1),
            _ => return false,
        }
        true
    }

    fn ui_elements(&self) -> Vec<UiElement> {
        let mut v: Vec<UiElement> = self
            .chips
            .iter()
            .map(|c| UiElement::new(format!("online_lobby.{}", c.name), c.at))
            .collect();
        if self.unsupported {
            return v;
        }
        let area = self.area;
        if self.in_guild() {
            for (i, m) in self
                .members
                .iter()
                .enumerate()
                .skip(self.first)
                .take(ROSTER_ROWS)
            {
                v.push(UiElement::new(
                    format!("online_lobby.guild_member({})", shown(&m.who)),
                    Rect::new(
                        area.x,
                        self.row_top(i - self.first),
                        super::friends_panel::ROWS_RIGHT as u16,
                        ROW_H as u16,
                    ),
                ));
            }
            v.push(UiElement::new(
                "online_lobby.guild.text",
                self.field_rect(TEXT_BOTTOM),
            ));
        } else {
            for (i, g) in self.invites.iter().take(INVITE_ROWS).enumerate() {
                v.push(UiElement::new(
                    format!("online_lobby.guild_invite({})", g.tag),
                    Rect::new(area.x, self.invite_top(i), area.w - 2, ROW_H as u16),
                ));
            }
            v.push(UiElement::new(
                "online_lobby.guild.tag",
                self.field_rect(TAG_BOTTOM),
            ));
            v.push(UiElement::new(
                "online_lobby.guild.name",
                self.field_rect(NAME_BOTTOM),
            ));
        }
        v
    }
}

#[cfg(test)]
mod tests;

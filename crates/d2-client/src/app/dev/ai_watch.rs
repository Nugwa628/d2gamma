// D2RUST's own: dev tools.
//! The monster AI overlay's reading of the game (`aiview`): where the
//! monsters that run a mod's behaviour graph stand in it, nearest to the
//! player first, with the transition they last made and the phase they are
//! in. It runs on the server thread between two frames, like every dev
//! command; the overlay only shows the lines.

use std::collections::BTreeMap;

use bevy::prelude::*;

use d2_sim::monsters::ai::graph::{GraphEvent, GraphEventKind, GraphRun};
use d2_sim::rules::data_mod::ai::AiDef;
use d2_sim::rules::data_mod::AiRules;
use d2_sim::units::UnitId;

use crate::app::single_player::Sim;

/// Monsters shown, nearest first.
pub const SHOWN: usize = 6;

/// A monster's last state change.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Moved {
    frame: i32,
    from: u8,
    to: u8,
}

/// A monster's last wind-up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Wound {
    event: u8,
}

/// One monster as the overlay shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AiSeen {
    pub guid: u32,
    pub class: u32,
    /// The larger of the two axes to the player, in sub-tiles.
    pub dist: i32,
    pub line: String,
}

/// What the watcher remembers of each monster: the events it was told of.
#[derive(Debug, Default)]
pub struct Watcher {
    moved: BTreeMap<UnitId, Moved>,
    wound: BTreeMap<UnitId, Wound>,
}

impl Watcher {
    /// Takes note of the graph events since the last look.
    pub fn note(&mut self, events: &[GraphEvent]) {
        for e in events {
            match e.kind {
                GraphEventKind::StateChanged { from, to } => {
                    self.moved.insert(
                        e.unit,
                        Moved {
                            frame: e.frame,
                            from,
                            to,
                        },
                    );
                }
                GraphEventKind::WindUp { event, .. } => {
                    self.wound.insert(e.unit, Wound { event });
                }
                GraphEventKind::PhaseChanged { .. } => {}
            }
        }
    }

    /// The text for one monster that runs `run` of the graph `def` on
    /// game frame `frame`.
    pub fn describe(
        &self,
        unit: UnitId,
        run: &GraphRun,
        def: Option<&AiDef>,
        frame: i32,
    ) -> String {
        let state = |n: u8| -> String {
            def.and_then(|d| d.states.get(usize::from(n)))
                .map_or_else(|| format!("state {n}"), |s| s.name.clone())
        };
        let mut out = format!(
            "{}: {} for {} ticks",
            def.map_or_else(
                || format!("graph {}", run.ai),
                |d| if d.name.is_empty() {
                    format!("graph {}", d.index)
                } else {
                    d.name.clone()
                }
            ),
            state(run.state),
            (frame - run.entered).max(0)
        );
        match self.moved.get(&unit) {
            Some(m) => out.push_str(&format!(
                "; last {} > {} {} ticks ago",
                state(m.from),
                state(m.to),
                (frame - m.frame).max(0)
            )),
            None => out.push_str("; no transition yet"),
        }
        if run.phase > 0 {
            let name = def
                .and_then(|d| d.phases.get(usize::from(run.phase) - 1))
                .map_or_else(|| format!("phase {}", run.phase), |p| p.name.clone());
            out.push_str(&format!("; phase {name}"));
        }
        if let Some(w) = run.windup {
            let name = self
                .wound
                .get(&unit)
                .and_then(|n| def.and_then(|d| d.events.get(usize::from(n.event))))
                .cloned()
                .unwrap_or_else(|| "an attack".into());
            out.push_str(&format!(
                "; winding up {name}, {} ticks left",
                (w.until - frame).max(0)
            ));
        }
        out
    }

    /// Looks at the game: the monsters that run a graph, nearest to
    /// `player` first. Takes the graph events the store holds.
    pub fn look(&mut self, s: &mut Sim, graphs: &AiRules, player: UnitId) -> Vec<AiSeen> {
        let frame = s.game.frame;
        let hooks = s.events.action.hooks();
        let Some(ai) = hooks.ai.as_mut() else {
            return Vec::new();
        };
        let events = ai.events.take();
        let runs = ai.graph_runs();
        self.note(&events);
        self.moved.retain(|u, _| runs.iter().any(|(r, _)| r == u));
        self.wound.retain(|u, _| runs.iter().any(|(r, _)| r == u));
        let (px, py) = hooks.path_position(player);
        let mut near: Vec<(i32, UnitId, GraphRun)> = runs
            .into_iter()
            .map(|(u, run)| {
                let (x, y) = hooks.path_position(u);
                ((x - px).abs().max((y - py).abs()), u, run)
            })
            .collect();
        near.sort_by_key(|(d, u, _)| (*d, *u));
        near.truncate(SHOWN);
        near.into_iter()
            .filter_map(|(dist, u, run)| {
                let guid = s.game.lists.unit(u)?.guid;
                let class = s.events.action.sys.units.get(u)?.class;
                let def = graphs.defs().iter().find(|d| d.index == run.ai);
                Some(AiSeen {
                    guid,
                    class,
                    dist,
                    line: self.describe(u, &run, def, frame),
                })
            })
            .collect()
    }
}

/// The overlay's text: one line per monster, or what to do when there is none.
pub fn text_of(seen: &[AiSeen]) -> String {
    if seen.is_empty() {
        return "no monster nearby runs a behaviour graph".into();
    }
    seen.iter()
        .map(|m| format!("#{} class {} ({} away) {}", m.guid, m.class, m.dist, m.line))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The monster AI overlay's switch (`aiview`). Off by default: nothing is
/// read from the game and nothing is drawn until it is on.
#[derive(Resource, Default)]
pub struct AiOverlay {
    pub on: bool,
}

#[derive(Component)]
pub(super) struct AiText;

pub(super) fn spawn_ai_view(mut commands: Commands) {
    commands.spawn((
        AiText,
        crate::app::session::GameEntity,
        Text::new(""),
        TextFont {
            font_size: bevy::text::FontSize::Px(12.0),
            ..default()
        },
        TextColor(Color::srgb(0.7, 1.0, 0.8)),
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.6)),
        Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(2.0),
            left: Val::Px(2.0),
            padding: UiRect::all(Val::Px(3.0)),
            ..default()
        },
        GlobalZIndex(1990),
        Visibility::Hidden,
    ));
}

pub(super) fn draw_ai_view(
    view: Res<AiOverlay>,
    overlay: Option<Res<super::overlay::DevOverlay>>,
    mut text: Query<(&mut Text, &mut Visibility), With<AiText>>,
) {
    for (mut t, mut v) in &mut text {
        let want = if view.on {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if *v != want {
            *v = want;
        }
        let (Some(o), true) = (overlay.as_deref(), view.on) else {
            continue;
        };
        let s = match o.chan.lock() {
            Ok(q) => text_of(&q.ai),
            Err(_) => continue,
        };
        if t.0 != s {
            t.0 = s;
        }
    }
}

#[cfg(test)]
mod tests;

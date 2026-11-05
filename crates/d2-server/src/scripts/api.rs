//! Narrow, integer-first capabilities borrowed only for one callback.
use super::{ScriptHost, Timer};
use d2_sim::combat::{damage::hitflag, hit::result, DamageRecord};
use d2_sim::rules::{RuledState, SkillHost};
use d2_sim::skills::use_::bodies::Summon;
use d2_sim::units::{UnitId, UnitType};
use mlua::{MultiValue, Value};

const NAMES: &[&str] = &[
    "stat",
    "base_stat",
    "set_stat",
    "has_state",
    "state_stacks",
    "apply_state",
    "add_stacks",
    "remove_stacks",
    "clear_state",
    "damage",
    "spawn_missile",
    "spawn_monster",
    "message",
    "ui_variable",
    "random",
    "timer",
];

fn error(message: &str) -> mlua::Error {
    mlua::Error::runtime(message)
}

pub(super) fn integer(value: &Value) -> mlua::Result<i32> {
    match value {
        Value::Integer(n) => i32::try_from(*n).map_err(mlua::Error::external),
        Value::Number(n)
            if n.is_finite()
                && n.fract() == 0.0
                && *n >= f64::from(i32::MIN)
                && *n <= f64::from(i32::MAX) =>
        {
            Ok(*n as i32)
        }
        _ => Err(error("expected an i32 integer")),
    }
}

/// The closures become invalid when the scope ends, even if Lua stores them.
pub(super) fn run(
    runtime: &ScriptHost,
    module: &str,
    m: &super::Module,
    function: &mlua::Function,
    host: &mut dyn SkillHost,
    event: d2_sim::rules::scripts::Event,
    call: Option<d2_sim::rules::scripts::CallContext>,
) -> mlua::Result<Value> {
    let host = std::cell::RefCell::new(host);
    let calls = std::cell::Cell::new(0_u16);
    m.lua.scope(|scope| {
        let api = m.lua.create_table()?;
        for &name in NAMES {
            let host = &host;
            let calls = &calls;
            api.raw_set(
                name,
                scope.create_function(move |_, args: MultiValue| {
                    let n = calls.get();
                    if n >= 128 {
                        return Err(error("script API call budget exceeded"));
                    }
                    calls.set(n + 1);
                    let mut host = host.borrow_mut();
                    invoke(runtime, module, &mut **host, name, args)
                })?,
            )?;
        }
        api.set_readonly(true);
        let context = m.lua.create_table()?;
        context.raw_set("version", d2_sim::rules::scripts::API_VERSION)?;
        context.raw_set(
            "event",
            if call.is_some() {
                "call"
            } else {
                event.kind.name()
            },
        )?;
        if let Some(call) = call {
            context.raw_set("skill", call.skill)?;
            context.raw_set("level", call.level)?;
            context.raw_set("argument", call.argument)?;
        }
        context.raw_set("frame", host.borrow().frame())?;
        context.raw_set("unit", event.unit.0)?;
        context.raw_set(
            "unit_type",
            host.borrow().unit_type(event.unit).map(|t| t as u8),
        )?;
        context.raw_set("other", event.other.map(|u| u.0))?;
        context.raw_set("id", event.id)?;
        context.set_readonly(true);
        function.call::<Value>((context, api))
    })
}

fn invoke(
    runtime: &ScriptHost,
    module: &str,
    h: &mut dyn SkillHost,
    name: &str,
    args: MultiValue,
) -> mlua::Result<Value> {
    let expected = match name {
        "stat" | "base_stat" | "remove_stacks" | "timer" | "ui_variable" => 3,
        "set_stat" | "damage" => 4,
        "has_state" | "state_stacks" | "clear_state" | "message" | "random" => 2,
        "apply_state" | "spawn_missile" => 5,
        "add_stacks" | "spawn_monster" => 6,
        _ => return Err(error("unknown API")),
    };
    if args.len() != expected {
        return Err(error("wrong number of API arguments"));
    }
    let int = |i: usize| integer(&args[i]);
    let unit = |i: usize| -> mlua::Result<UnitId> {
        let n = match &args[i] {
            Value::Integer(n) => u32::try_from(*n).ok(),
            Value::Number(n)
                if n.is_finite() && n.fract() == 0.0 && *n >= 0.0 && *n <= f64::from(u32::MAX) =>
            {
                Some(*n as u32)
            }
            _ => None,
        }
        .ok_or_else(|| error("expected a unit handle"))?;
        let u = UnitId(n);
        h.unit_type(u)
            .ok_or_else(|| error("unit is no longer in the game"))?;
        Ok(u)
    };
    let id = |i: usize, kind: &str| -> mlua::Result<u16> {
        let n = u16::try_from(int(i)?).map_err(mlua::Error::external)?;
        if !h.script_id_valid(kind, n) {
            return Err(error("unknown table ID"));
        }
        Ok(n)
    };
    let layer = |i| u16::try_from(int(i)?).map_err(mlua::Error::external);
    let value = |n: i32| Value::Integer(n.into());
    let u = unit(0)?;
    match name {
        "stat" => Ok(value(h.stat(u, id(1, "stat")?, layer(2)?))),
        "base_stat" => Ok(value(h.base_stat(u, id(1, "stat")?, layer(2)?))),
        "set_stat" => {
            h.set_stat_layer(u, id(1, "stat")?, layer(2)?, int(3)?);
            Ok(Value::Nil)
        }
        "has_state" => Ok(Value::Boolean(h.has_state(u, id(1, "state")?))),
        "state_stacks" => Ok(value(i32::from(h.state_stacks(u, id(1, "state")?)))),
        "clear_state" => {
            h.clear_state(u, id(1, "state")?);
            Ok(Value::Nil)
        }
        "remove_stacks" => {
            h.remove_state_stacks(u, id(1, "state")?, layer(2)?);
            Ok(Value::Nil)
        }
        "apply_state" | "add_stacks" => {
            let target = unit(1)?;
            let state = id(2, "state")?;
            let ticks = int(3)?;
            if !(0..=2_160_000).contains(&ticks) {
                return Err(error("state duration out of range"));
            }
            let skill = i32::from(id(4, "skill")?);
            let spec = RuledState {
                state,
                skill,
                level: 1,
                base_ticks: ticks,
            };
            if name == "apply_state" {
                h.apply_ruled_state(u, target, &spec);
            } else {
                h.add_state_stacks(u, target, &spec, layer(5)?);
            }
            Ok(Value::Nil)
        }
        "damage" => {
            let target = unit(1)?;
            let amount = int(2)?;
            let element = int(3)?;
            if amount < 0 || !(0..=5).contains(&element) {
                return Err(error("damage amount/element out of range"));
            }
            if !matches!(h.unit_type(u), Some(UnitType::Player | UnitType::Monster))
                || !matches!(
                    h.unit_type(target),
                    Some(UnitType::Player | UnitType::Monster)
                )
            {
                return Err(error("damage requires player or monster units"));
            }
            let mut rec = DamageRecord {
                result: result::HIT,
                hit_flags: hitflag::ROLLED,
                ..Default::default()
            };
            if element == 0 {
                rec.physical = amount;
            } else {
                h.add_element(u, &mut rec, element, amount, 25);
            }
            h.apply_damage(u, target, true, &mut rec);
            h.react(u, target, &mut rec);
            Ok(Value::Nil)
        }
        "spawn_missile" => {
            let missile = i32::from(id(1, "missile")?);
            let skill = i32::from(id(2, "skill")?);
            let level = int(3)?;
            let lob = int(4)?;
            if !(1..=255).contains(&level) || !(0..=1).contains(&lob) {
                return Err(error("missile level/lob out of range"));
            }
            Ok(h.fire_skill_missile(u, missile, skill, level, lob != 0)
                .map_or(Value::Nil, |u| Value::Number(f64::from(u.0))))
        }
        "spawn_monster" => {
            let class = i32::from(id(1, "monster")?);
            let x = int(2)?;
            let y = int(3)?;
            let pet_type = int(4)?;
            let max = int(5)?;
            if !(0..=18).contains(&pet_type)
                || !(1..=32).contains(&max)
                || !(0..=65535).contains(&x)
                || !(0..=65535).contains(&y)
            {
                return Err(error("monster position/pet limits out of range"));
            }
            let pet = h.summon(Summon {
                flags: 1,
                owner: u,
                class,
                ai: 0,
                mode: 1,
                x,
                y,
                pet_type,
                pet_max: max,
            });
            if let Some(pet) = pet {
                h.pet_base_stats(u, pet, 0, 1);
                h.follow_owner(u, pet);
            }
            Ok(pet.map_or(Value::Nil, |u| Value::Number(f64::from(u.0))))
        }
        "ui_variable" => {
            let Value::String(s) = &args[1] else {
                return Err(error("UI variable name must be a string"));
            };
            let s = s.to_str()?;
            let name = format!("{}/{}", module.strip_suffix(".luau").unwrap_or(module), s);
            if !d2_proto::mod_ui::valid_name(&s)
                || !d2_proto::mod_ui::valid_name(&name)
                || h.unit_type(u) != Some(UnitType::Player)
            {
                return Err(error(
                    "UI variable requires a player and a valid namespaced name",
                ));
            }
            h.script_ui_variable(u, &name, int(2)?);
            Ok(Value::Nil)
        }
        "message" => {
            let Value::String(s) = &args[1] else {
                return Err(error("message must be a string"));
            };
            let s = s.to_str()?;
            if s.len() > 200 || s.bytes().any(|b| !(32..=126).contains(&b)) {
                return Err(error("message must be at most 200 printable ASCII bytes"));
            }
            if h.unit_type(u) != Some(UnitType::Player) {
                return Err(error("message requires a player"));
            }
            h.script_message(u, &s);
            Ok(Value::Nil)
        }
        "random" => {
            let upper = int(1)?;
            if upper < 1 {
                return Err(error("random upper bound must be positive"));
            }
            Ok(value(h.seed(u).roll(upper) as i32))
        }
        "timer" => {
            let delay = int(1)?;
            let cookie = int(2)?;
            if !(1..=2_160_000).contains(&delay) {
                return Err(error("timer delay must be 1..2160000 frames"));
            }
            let at = h
                .frame()
                .checked_add(delay)
                .ok_or_else(|| error("timer deadline overflow"))?;
            let mut timers = runtime
                .timers
                .lock()
                .map_err(|_| error("timer store unavailable"))?;
            if timers.len() >= 256 {
                return Err(error("script timer limit exceeded"));
            }
            timers.push(Timer {
                module: module.to_owned(),
                unit: u,
                at,
                cookie,
            });
            Ok(Value::Nil)
        }
        _ => Err(error("unknown API")),
    }
}

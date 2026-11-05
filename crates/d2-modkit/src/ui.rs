//! Versioned mod panel documents. Coordinates use the 1280 by 720 view;
//! presentation scaling belongs to the client, never to game rules.

use std::collections::BTreeSet;
use toml::{Table, Value};

/// Built-in panel names shared by the loader and authoring tools.
pub const BUILTIN_PANELS: &[&str] = &[
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
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Panel {
    pub name: String,
    pub replace: Option<String>,
    pub elements: Vec<Element>,
    pub quests: Vec<QuestFlag>,
    pub actions: Vec<Action>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Element {
    pub id: String,
    pub kind: Kind,
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
    pub color: u8,
    pub text: String,
    /// Panel-art name under data/global/ui, without the DC6 extension.
    pub image: String,
    pub frame: u32,
    pub value: u16,
    pub binding: Option<Binding>,
    pub action: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuestFlag {
    pub name: String,
    pub slot: u8,
    pub bit: u8,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Action {
    pub name: String,
    pub stat: String,
    pub delta: i32,
    pub minimum: i32,
    pub maximum: i32,
    pub cooldown: u16,
    pub town: bool,
    pub cost_stat: Option<String>,
    pub cost: i32,
    pub validator: Option<Validator>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Validator {
    pub module: String,
    pub function: String,
}

impl Action {
    pub fn parse(mut t: Table) -> Result<Self, String> {
        let action_name = string(&mut t, "name", None)?;
        let stat = string(&mut t, "stat", None)?;
        let delta = signed(&mut t, "delta", 0)?;
        let minimum = signed(&mut t, "minimum", 0)?;
        let maximum = signed(&mut t, "maximum", i32::MAX)?;
        let cooldown = number(&mut t, "cooldown", 25, 3600)?;
        let town = match t.remove("town").unwrap_or(Value::Boolean(false)) {
            Value::Boolean(v) => v,
            _ => return Err("action.town: expected boolean".into()),
        };
        let cost_stat = string(&mut t, "cost_stat", Some(""))?;
        let cost = signed(&mut t, "cost", 0)?;
        if !name(&action_name)
            || !source(&format!("stat.{stat}"))
            || delta == 0
            || minimum > maximum
            || cooldown == 0
        {
            return Err("action: invalid name/stat, zero delta/cooldown or reversed bounds".into());
        }
        if (cost_stat.is_empty() && cost != 0)
            || (!cost_stat.is_empty() && (cost <= 0 || !source(&format!("stat.{cost_stat}"))))
        {
            return Err("action: cost_stat needs a positive cost; cost needs cost_stat".into());
        }
        let validator = match t.remove("validator") {
            None => None,
            Some(Value::Table(mut v)) => {
                let module = string(&mut v, "module", None)?;
                let function = string(&mut v, "function", None)?;
                exhausted(&v)?;
                if !module.ends_with(".luau")
                    || module.contains("..")
                    || module.starts_with('/')
                    || !source(&format!("var.{module}"))
                    || module.split('/').any(str::is_empty)
                    || !function.bytes().enumerate().all(|(i, b)| {
                        b == b'_' || b.is_ascii_alphabetic() || (i > 0 && b.is_ascii_digit())
                    })
                    || function.is_empty()
                {
                    return Err("action.validator: expected module.luau and function name".into());
                }
                Some(Validator { module, function })
            }
            _ => return Err("action.validator: expected table".into()),
        };
        exhausted(&t)?;
        Ok(Self {
            name: action_name,
            stat,
            delta,
            minimum,
            maximum,
            cooldown,
            town,
            cost_stat: (!cost_stat.is_empty()).then_some(cost_stat),
            cost,
            validator,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Binding {
    pub source: String,
    pub maximum_source: Option<String>,
    pub minimum: i32,
    pub maximum: i32,
    pub divisor: i32,
    pub format: Format,
    pub colors: Vec<ColorRange>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Integer,
    Percent,
    Range,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ColorRange {
    pub minimum: i32,
    pub maximum: i32,
    pub color: u8,
}

fn signed(t: &mut Table, key: &str, default: i32) -> Result<i32, String> {
    match t.remove(key).unwrap_or(Value::Integer(i64::from(default))) {
        Value::Integer(n) => i32::try_from(n).map_err(|_| format!("{key}: expected i32")),
        _ => Err(format!("{key}: expected i32")),
    }
}

fn source(s: &str) -> bool {
    let Some((kind, key)) = s.split_once('.') else {
        return false;
    };
    matches!(kind, "stat" | "state" | "stacks" | "var" | "quest" | "sync")
        && !key.is_empty()
        && key.len() <= 128
        && key
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_./-".contains(&b))
}

impl Binding {
    fn parse(mut t: Table) -> Result<Self, String> {
        let source_name = string(&mut t, "source", None)?;
        let maximum_source = string(&mut t, "maximum_source", Some(""))?;
        if !source(&source_name) || (!maximum_source.is_empty() && !source(&maximum_source)) {
            return Err(
                "binding: expected stat., state., stacks., var., quest. or sync. followed by a name"
                    .into(),
            );
        }
        let minimum = signed(&mut t, "minimum", 0)?;
        let maximum = signed(&mut t, "maximum", 1000)?;
        let divisor = signed(&mut t, "divisor", 1)?;
        if maximum <= minimum || divisor <= 0 {
            return Err("binding: maximum must exceed minimum; divisor must be positive".into());
        }
        let format = match string(&mut t, "format", Some("integer"))?.as_str() {
            "integer" => Format::Integer,
            "percent" => Format::Percent,
            "range" => Format::Range,
            _ => return Err("binding: unknown format".into()),
        };
        let mut colors = Vec::new();
        if let Some(v) = t.remove("colors") {
            let Value::Array(a) = v else {
                return Err("binding.colors: expected array".into());
            };
            if a.len() > 16 {
                return Err("binding.colors: at most 16 ranges".into());
            }
            for v in a {
                let Value::Table(mut c) = v else {
                    return Err("binding.colors: expected tables".into());
                };
                let minimum = signed(&mut c, "minimum", i32::MIN)?;
                let maximum = signed(&mut c, "maximum", i32::MAX)?;
                let color = number(&mut c, "color", 255, 255)? as u8;
                exhausted(&c)?;
                if minimum > maximum {
                    return Err("binding.colors: reversed range".into());
                }
                colors.push(ColorRange {
                    minimum,
                    maximum,
                    color,
                });
            }
        }
        exhausted(&t)?;
        Ok(Self {
            source: source_name,
            maximum_source: (!maximum_source.is_empty()).then_some(maximum_source),
            minimum,
            maximum,
            divisor,
            format,
            colors,
        })
    }

    /// Saturating integer arithmetic keeps hostile values from overflowing UI geometry.
    pub fn present(&self, raw: i32, raw_max: Option<i32>, fallback_color: u8) -> (String, u16, u8) {
        let value = i64::from(raw) / i64::from(self.divisor);
        let maximum = raw_max
            .map(|v| i64::from(v) / i64::from(self.divisor))
            .unwrap_or(i64::from(self.maximum));
        let minimum = i64::from(self.minimum);
        let fraction = if maximum > minimum {
            ((value - minimum) * 1000 / (maximum - minimum)).clamp(0, 1000)
        } else {
            0
        };
        let text = match self.format {
            Format::Integer => value.to_string(),
            Format::Percent => format!("{}%", fraction / 10),
            Format::Range => format!("{value}/{maximum}"),
        };
        let color = self
            .colors
            .iter()
            .find(|c| (i64::from(c.minimum)..=i64::from(c.maximum)).contains(&value))
            .map_or(fallback_color, |c| c.color);
        (text, fraction as u16, color)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Frame,
    Text,
    Bar,
    Button,
    Image,
}

fn string(t: &mut Table, key: &str, default: Option<&str>) -> Result<String, String> {
    match (t.remove(key), default) {
        (Some(Value::String(s)), _) => Ok(s),
        (None, Some(value)) => Ok(value.into()),
        _ => Err(format!("{key}: expected a string")),
    }
}

fn number(t: &mut Table, key: &str, default: i64, max: i64) -> Result<u16, String> {
    match t.remove(key).unwrap_or(Value::Integer(default)) {
        Value::Integer(n) if (0..=max).contains(&n) => Ok(n as u16),
        _ => Err(format!("{key}: expected an integer from 0 to {max}")),
    }
}

fn exhausted(t: &Table) -> Result<(), String> {
    match t.keys().next() {
        Some(k) => Err(format!("unknown field {k}")),
        None => Ok(()),
    }
}

fn name(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

impl Panel {
    pub fn parse(text: &str) -> Result<Self, String> {
        if text.len() > 256 * 1024 {
            return Err("panel exceeds 256 KiB".into());
        }
        let mut t: Table = text.parse().map_err(|e| format!("{e}"))?;
        if number(&mut t, "version", 0, 65535)? != 1 {
            return Err("version: expected 1; this mod needs updating".into());
        }
        let panel_name = string(&mut t, "name", None)?;
        if !name(&panel_name) {
            return Err("name: expected 1–64 letters, digits, _ or -".into());
        }
        let replace = string(&mut t, "replace", Some(""))?;
        if !replace.is_empty() && !BUILTIN_PANELS.contains(&replace.as_str()) {
            return Err(format!("replace: unknown built-in panel {replace}"));
        }
        let values = match t.remove("elements") {
            Some(Value::Array(a)) if a.len() <= 256 => a,
            None => Vec::new(),
            _ => return Err("elements: expected at most 256 elements".into()),
        };
        let mut quests = Vec::new();
        let mut quest_names = BTreeSet::new();
        if let Some(v) = t.remove("quests") {
            let Value::Array(a) = v else {
                return Err("quests: expected array".into());
            };
            if a.len() > 256 {
                return Err("quests: at most 256 names".into());
            }
            for v in a {
                let Value::Table(mut q) = v else {
                    return Err("quests: expected tables".into());
                };
                let name = string(&mut q, "name", None)?;
                if !source(&format!("quest.{name}")) || !quest_names.insert(name.clone()) {
                    return Err("quests: invalid or duplicate name".into());
                }
                let slot = number(&mut q, "slot", 0, 47)? as u8;
                let bit = number(&mut q, "bit", 0, 15)? as u8;
                exhausted(&q)?;
                quests.push(QuestFlag { name, slot, bit });
            }
        }
        let mut actions = Vec::new();
        let mut action_names = BTreeSet::new();
        if let Some(v) = t.remove("actions") {
            let Value::Array(a) = v else {
                return Err("actions: expected array".into());
            };
            if a.len() > 32 {
                return Err("actions: at most 32 declarations per panel".into());
            }
            for v in a {
                let Value::Table(t) = v else {
                    return Err("actions: expected tables".into());
                };
                let a = Action::parse(t)?;
                if panel_name.len() + a.name.len() + 1 > 128 || !action_names.insert(a.name.clone())
                {
                    return Err("actions: duplicate or overlong action name".into());
                }
                actions.push(a);
            }
        }
        exhausted(&t)?;
        let mut ids = BTreeSet::new();
        let mut elements = Vec::new();
        for v in values {
            let Value::Table(mut t) = v else {
                return Err("elements: expected tables".into());
            };
            let id = string(&mut t, "id", None)?;
            if !name(&id) || !ids.insert(id.clone()) {
                return Err(format!("element {id}: invalid or duplicate id"));
            }
            let kind = match string(&mut t, "kind", None)?.as_str() {
                "frame" => Kind::Frame,
                "text" => Kind::Text,
                "bar" => Kind::Bar,
                "button" => Kind::Button,
                "image" => Kind::Image,
                k => return Err(format!("element {id}: unknown kind {k}")),
            };
            let x = number(&mut t, "x", 0, 1279)?;
            let y = number(&mut t, "y", 0, 719)?;
            let width = number(&mut t, "width", 100, 1280)?;
            let height = number(&mut t, "height", 20, 720)?;
            if width == 0 || height == 0 || x + width > 1280 || y + height > 720 {
                return Err(format!("element {id}: rectangle leaves the 1280x720 view"));
            }
            let color = number(&mut t, "color", 255, 255)? as u8;
            let text = string(&mut t, "text", Some(""))?;
            let image = string(&mut t, "image", Some(""))?;
            if kind == Kind::Image
                && (image.is_empty()
                    || !image.is_ascii()
                    || image.contains("..")
                    || image.contains(':')
                    || image.starts_with(['/', '\\']))
            {
                return Err(format!(
                    "element {id}: expected a relative panel-art image name"
                ));
            }
            let frame = u32::from(number(&mut t, "frame", 0, 65535)?);
            let value = number(&mut t, "value", 1000, 1000)?;
            let binding = match t.remove("bind") {
                Some(Value::Table(t)) => Some(Binding::parse(t)?),
                None => None,
                _ => return Err(format!("element {id}: bind must be a table")),
            };
            if matches!(kind, Kind::Text | Kind::Button) {
                if let Some(b) = &binding {
                    if (color > 12 && color != 255) || b.colors.iter().any(|c| c.color > 12) {
                        return Err(format!(
                            "element {id}: bound text colors must be 0..12 (255 uses plain text)"
                        ));
                    }
                }
            }
            let action = string(&mut t, "action", Some(""))?;
            if !action.is_empty()
                && (kind != Kind::Button
                    || (!action_names.contains(&action)
                        && !action.strip_prefix("net.").is_some_and(name)))
            {
                return Err(format!(
                    "element {id}: action must name a declared button action"
                ));
            }
            exhausted(&t).map_err(|e| format!("element {id}: {e}"))?;
            elements.push(Element {
                id,
                kind,
                x,
                y,
                width,
                height,
                color,
                text,
                image,
                frame,
                value,
                binding,
                action: (!action.is_empty()).then_some(action),
            });
        }
        Ok(Self {
            name: panel_name,
            replace: (!replace.is_empty()).then_some(replace),
            elements,
            quests,
            actions,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn binding_formats_ranges_colors_and_extreme_values() {
        let p = Panel::parse("version=1\nname='hud'\n[[elements]]\nid='hp'\nkind='bar'\n[elements.bind]\nsource='stat.hp'\nmaximum_source='stat.maxhp'\nformat='percent'\ndivisor=256\n[[elements.bind.colors]]\nmaximum=25\ncolor=3").unwrap();
        let mut b = p.elements[0].binding.clone().unwrap();
        assert_eq!(
            b.present(25 * 256, Some(100 * 256), 9),
            ("25%".into(), 250, 3)
        );
        b.format = Format::Range;
        assert_eq!(
            b.present(50 * 256, Some(100 * 256), 9),
            ("50/100".into(), 500, 9)
        );
        assert_eq!(b.present(i32::MAX, Some(i32::MIN), 9).1, 0);
        b.divisor = 1;
        b.minimum = i32::MIN;
        b.maximum = i32::MAX;
        assert_eq!(b.present(i32::MAX, None, 9).1, 1000);
        for bad in [
            "source='typo.x'",
            "source='stat.x'\ndivisor=0",
            "source='stat.x'\nminimum=5\nmaximum=5",
        ] {
            assert!(Panel::parse(&format!(
                "version=1\nname='x'\n[[elements]]\nid='a'\nkind='text'\n[elements.bind]\n{bad}"
            ))
            .is_err());
        }
    }
    #[test]
    fn strict_version_bounds_and_ids() {
        let s = "version=1\nname='hud'\n[[elements]]\nid='a'\nkind='bar'\n";
        assert_eq!(Panel::parse(s).unwrap().elements[0].value, 1000);
        for bad in [
            s.replace("version=1", "version=2"),
            format!("{s}typo=1\n"),
            format!("{s}x=1280\n"),
            format!("{s}[[elements]]\nid='a'\nkind='text'\n"),
        ] {
            assert!(Panel::parse(&bad).is_err(), "{bad}");
        }
    }
    #[test]
    fn declarations_reject_unbounded_or_undeclared_actions_and_keep_defaults_empty() {
        assert!(Panel::parse("version=1\nname='plain'")
            .unwrap()
            .actions
            .is_empty());
        let base =
            "version=1\nname='hud'\n[[actions]]\nname='allocate'\nstat='strength'\ndelta=1\n";
        for extra in [
            "cooldown=0",
            "delta=0",
            "minimum=2\nmaximum=1",
            "cost=1",
            "cost_stat='points'",
            "town=1",
            "unknown=1",
            "[actions.validator]\nmodule='../escape.luau'\nfunction='allow'",
        ] {
            let text = if extra == "delta=0" {
                base.replace("delta=1", extra)
            } else {
                format!("{base}{extra}")
            };
            assert!(Panel::parse(&text).is_err(), "{text}");
        }
        for (kind, action) in [("text", "allocate"), ("button", "typo")] {
            assert!(Panel::parse(&format!(
                "{base}[[elements]]\nid='a'\nkind='{kind}'\naction='{action}'"
            ))
            .is_err());
        }
        let p=Panel::parse(&format!("{base}cost_stat='points'\ncost=1\n[actions.validator]\nmodule='test.luau'\nfunction='allow'")).unwrap();
        assert_eq!((p.actions[0].cooldown, p.actions[0].cost), (25, 1));
    }
    #[test]
    fn replacement_names_are_shared_with_authoring_tools() {
        for name in BUILTIN_PANELS {
            assert!(Panel::parse(&format!("version=1\nname='custom'\nreplace='{name}'")).is_ok());
        }
        assert!(Panel::parse("version=1\nname='custom'\nreplace='inventroy'").is_err());
    }
}

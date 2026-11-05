//! Bounded visual recipes. Import conversion is an authoring operation.
use std::collections::BTreeSet;
use toml::{Table, Value};
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Effect {
    pub name: String,
    pub duration: u16,
    pub missile: Option<String>,
    pub overlay: Option<String>,
    pub light: Option<Light>,
    pub particles: Option<Particles>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Light {
    pub radius: u8,
    pub red: u8,
    pub green: u8,
    pub blue: u8,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Particles {
    pub count: u16,
    pub speed: u16,
    pub spread: u16,
    pub gravity: i16,
    pub color: u8,
}
pub fn parse(text: &str) -> Result<Vec<Effect>, String> {
    if text.len() > 256 * 1024 {
        return Err("effects: document exceeds 256 KiB".into());
    }
    let mut t: Table = text.parse().map_err(|e| format!("effects: {e}"))?;
    if number(&mut t, "version", 0, 1, 1)? != 1 {
        return Err("effects: version must be 1".into());
    }
    let a = match t.remove("effects").unwrap_or(Value::Array(Vec::new())) {
        Value::Array(a) => a,
        _ => return Err("effects: expected array".into()),
    };
    exhausted(&t)?;
    if a.len() > 128 {
        return Err("effects: at most 128 recipes per document".into());
    }
    let mut names = BTreeSet::new();
    let mut out = Vec::new();
    for v in a {
        let Value::Table(mut t) = v else {
            return Err("effect: expected table".into());
        };
        let name = string(&mut t, "name")?.ok_or("effect: name is required")?;
        if !valid_name(&name) || !names.insert(name.clone()) {
            return Err("effect: invalid or duplicate name".into());
        }
        let duration = number(&mut t, "duration", 25, 1, 3600)? as u16;
        let missile = string(&mut t, "missile")?;
        let overlay = string(&mut t, "overlay")?;
        for n in missile.iter().chain(overlay.iter()) {
            if n.is_empty() || n.len() > 128 || !n.is_ascii() {
                return Err("effect: invalid table name".into());
            }
        }
        let light = child(&mut t, "light", |mut t| {
            let v = Light {
                radius: number(&mut t, "radius", 8, 1, 18)? as u8,
                red: number(&mut t, "red", 255, 0, 255)? as u8,
                green: number(&mut t, "green", 180, 0, 255)? as u8,
                blue: number(&mut t, "blue", 80, 0, 255)? as u8,
            };
            exhausted(&t)?;
            Ok(v)
        })?;
        let particles = child(&mut t, "particles", |mut t| {
            let v = Particles {
                count: number(&mut t, "count", 16, 1, 128)? as u16,
                speed: number(&mut t, "speed", 4, 0, 64)? as u16,
                spread: number(&mut t, "spread", 32, 0, 512)? as u16,
                gravity: number(&mut t, "gravity", 0, -64, 64)? as i16,
                color: number(&mut t, "color", 255, 1, 255)? as u8,
            };
            exhausted(&t)?;
            Ok(v)
        })?;
        exhausted(&t)?;
        if missile.is_none() && overlay.is_none() && light.is_none() && particles.is_none() {
            return Err("effect: at least one visual component is required".into());
        }
        out.push(Effect {
            name,
            duration,
            missile,
            overlay,
            light,
            particles,
        });
    }
    Ok(out)
}
pub fn valid_name(n: &str) -> bool {
    !n.is_empty()
        && n.len() <= 128
        && n.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_./-".contains(&b))
}
fn string(t: &mut Table, k: &str) -> Result<Option<String>, String> {
    match t.remove(k) {
        None => Ok(None),
        Some(Value::String(s)) => Ok(Some(s)),
        _ => Err(format!("{k}: expected string")),
    }
}
fn number(t: &mut Table, k: &str, d: i64, min: i64, max: i64) -> Result<i64, String> {
    match t.remove(k).unwrap_or(Value::Integer(d)) {
        Value::Integer(n) if (min..=max).contains(&n) => Ok(n),
        _ => Err(format!("{k}: expected {min}..{max}")),
    }
}
fn child<T>(
    t: &mut Table,
    k: &str,
    f: impl FnOnce(Table) -> Result<T, String>,
) -> Result<Option<T>, String> {
    match t.remove(k) {
        None => Ok(None),
        Some(Value::Table(t)) => f(t).map(Some),
        _ => Err(format!("{k}: expected table")),
    }
}
fn exhausted(t: &Table) -> Result<(), String> {
    if let Some(k) = t.keys().next() {
        Err(format!("unknown effect key {k}"))
    } else {
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn default_is_empty_and_recipes_have_bounded_components() {
        assert!(parse("version=1").unwrap().is_empty());
        let s="version=1\n[[effects]]\nname='ember'\nmissile='arrow'\noverlay='glow'\n[effects.light]\nradius=12\n[effects.particles]\ncount=32";
        let e = parse(s).unwrap();
        assert_eq!(e[0].duration, 25);
        assert_eq!(e[0].particles.as_ref().unwrap().count, 32);
        for bad in [
            s.replace("version=1", "version=2"),
            s.replace("radius=12", "radius=19"),
            s.replace("count=32", "count=129"),
            format!("{s}\ntypo=1"),
            "version=1\n[[effects]]\nname='empty'".into(),
            s.replace("name='ember'", "name='bad name'"),
        ] {
            assert!(parse(&bad).is_err(), "{bad}");
        }
    }
}

/// Screen-pixel offsets at the engine's base 25 Hz, shared with Studio preview.
/// The recipe and particle index are the entire seed; no game RNG is consumed.
pub fn particle_position(p: &Particles, index: u16, age: u16) -> (i32, i32) {
    let seed = u32::from(index)
        .wrapping_mul(1664525)
        .wrapping_add(1013904223);
    let spread = i64::from(p.spread);
    let age = i64::from(age.min(3600));
    let start = |n: u32| i64::from(n % (p.spread as u32 * 2 + 1)) - spread;
    let dx = i64::from(((seed >> 8) % 17) as i32 - 8);
    let dy = i64::from(((seed >> 16) % 17) as i32 - 8);
    let speed = i64::from(p.speed);
    (
        (start(seed) + dx * speed * age / 200) as i32,
        (start(seed.rotate_left(13))
            + dy * speed * age / 200
            + i64::from(p.gravity) * age * age / 1250) as i32,
    )
}

#[cfg(test)]
mod particle_tests {
    use super::*;
    #[test]
    fn particles_are_deterministic_and_bounded_without_shared_rng() {
        let p = Particles {
            count: 128,
            speed: 64,
            spread: 512,
            gravity: 64,
            color: 1,
        };
        for i in 0..128 {
            assert_eq!(
                particle_position(&p, i, 3600),
                particle_position(&p, i, u16::MAX)
            );
            let (x, y) = particle_position(&p, i, 3600);
            assert!(x.abs() < 10_000 && y.abs() < 700_000);
        }
        assert_ne!(particle_position(&p, 0, 0), particle_position(&p, 1, 0));
    }
    #[test]
    fn every_component_setting_rejects_invalid_values() {
        let base = "version=1\n[[effects]]\nname='test'\n[effects.particles]\n";
        for (k, n) in [
            ("count", 0),
            ("speed", 65),
            ("spread", 513),
            ("gravity", -65),
            ("color", 0),
        ] {
            assert!(parse(&format!("{base}{k}={n}")).is_err());
        }
        for (k, n) in [("red", 256), ("green", -1), ("blue", 256), ("radius", 0)] {
            assert!(parse(&format!(
                "version=1\n[[effects]]\nname='test'\n[effects.light]\n{k}={n}"
            ))
            .is_err());
        }
        assert!(parse("[[effects]]\nname='test'\nmissile='a'").is_err());
    }
    #[test]
    fn effect_recipe_limits_duplicates_and_duration_are_strict() {
        let recipe =
            |n| format!("[[effects]]\nname='effect_{n}'\nduration=3600\n[effects.light]\n");
        let max = format!("version=1\n{}", (0..128).map(recipe).collect::<String>());
        assert_eq!(parse(&max).unwrap().len(), 128);
        assert!(parse(&format!("{max}{}", recipe(128))).is_err());
        assert!(parse(&format!("{max}{}", recipe(0))).is_err());
        assert!(parse(&format!("version=1\n{}", " ".repeat(256 * 1024))).is_err());
        for n in [0, 3601] {
            assert!(parse(&format!(
                "version=1\n[[effects]]\nname='a'\nduration={n}\n[effects.light]"
            ))
            .is_err());
        }
        assert!(parse("version=1\n[[effects]]\nname='a'\n[effects.light]\ntypo=1").is_err());
    }
}

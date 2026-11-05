//! Friendly authoring labels. Raw keys remain visible on hover and unknown keys
//! retain their exact spelling so custom columns remain usable.
pub fn label<'a>(table: &str, key: &'a str) -> &'a str {
    let common = match key.to_ascii_lowercase().as_str() {
        "durability" => Some("Durability"),
        "minstack" => Some("Minimum stack size"),
        "maxstack" => Some("Maximum stack size"),
        "spawnstack" => Some("Dropped stack size"),
        "type2" => Some("Secondary item type"),
        "normcode" => Some("Normal base code"),
        "ubercode" => Some("Exceptional base code"),
        "ultracode" => Some("Elite base code"),
        "minmisdam" => Some("Minimum missile damage"),
        "maxmisdam" => Some("Maximum missile damage"),
        "strbonus" => Some("Strength damage bonus"),
        "dexbonus" => Some("Dexterity damage bonus"),
        "maxsock1" => Some("Sockets at low item levels"),
        "maxsock25" => Some("Sockets at medium item levels"),
        "maxsock40" => Some("Sockets at high item levels"),
        "compactsave" => Some("Use compact save format"),
        "bitfield1" => Some("Item flags"),
        "component" => Some("Equipment appearance slot"),
        "flippyfile" => Some("Ground drop animation"),
        "invfile" => Some("Inventory image"),
        "uniqueinvfile" => Some("Unique inventory image"),
        "setinvfile" => Some("Set inventory image"),
        "alternategfx" => Some("Equipment appearance code"),
        "dropsound" => Some("Drop sound"),
        "dropsfxframe" => Some("Drop sound frame"),
        "usesound" => Some("Use sound"),
        "costadd" => Some("Added gold value"),
        "gamble cost" => Some("Gambling cost"),
        "gamblecost" => Some("Gambling cost"),
        "invtrans" => Some("Inventory color transform"),
        "transform" => Some("Equipment color transform"),
        "belt" => Some("Belt capacity type"),
        "quest" => Some("Quest item"),
        "questdiffcheck" => Some("Check quest difficulty"),
        "unique" => Some("Always unique"),
        "useable" => Some("Can use"),
        "throwable" => Some("Can throw"),
        "2handed" => Some("Two handed"),
        "1or2handed" => Some("Can wield in one or two hands"),
        "wclass" => Some("Weapon animation class"),
        "2handedwclass" => Some("Two handed animation class"),
        "hit class" => Some("Hit sound class"),
        "missiletype" => Some("Thrown missile type"),
        "spelldesc" => Some("Use description mode"),
        "spelldescstr" => Some("Use description text key"),
        "spelldesccalc" => Some("Use description formula"),
        "autoprefix" => Some("Automatic affix group"),
        "magic lvl" => Some("Bonus magic level"),
        "levelreq" => Some("Required level"),
        "code" => Some("Item code"),
        "name" => Some("Name"),
        "namestr" => Some("Display name key"),
        "gemsockets" => Some("Maximum sockets"),
        "gemapplytype" => Some("Socket bonus type"),
        "hasinv" => Some("Can have sockets"),
        "invwidth" => Some("Inventory width"),
        "invheight" => Some("Inventory height"),
        "cost" => Some("Gold value"),
        "speed" => Some("Attack speed modifier"),
        "rangeadder" => Some("Extra melee reach"),
        "nodurability" => Some("No durability loss"),
        "stackable" => Some("Can stack"),
        "index" => Some("Name key"),
        "level" => Some("Item level"),
        "enabled" => Some("Enabled"),
        "spawnable" => Some("Can drop"),
        "rarity" => Some("Rarity weight"),
        "version" => Some("Game version"),
        _ => None,
    };
    if let Some(label) = common {
        return label;
    }
    match (table, key.to_ascii_lowercase().as_str()) {
        ("weapons", "mindam") => "Min damage",
        ("weapons", "maxdam") => "Max damage",
        ("weapons", "2handmindam") => "Two-handed min damage",
        ("weapons", "2handmaxdam") => "Two-handed max damage",
        ("weapons" | "armor", "reqstr") => "Required strength",
        ("weapons" | "armor", "reqdex") => "Required dexterity",
        ("weapons" | "armor", "durability") => "Durability",
        ("armor", "minac") => "Min defense",
        ("armor", "maxac") => "Max defense",
        ("weapons" | "armor" | "misc", "type") => "Item type",
        ("uniqueitems" | "setitems", "code" | "item") => "Base item",
        ("uniqueitems", "lvl req") => "Required level",
        ("uniqueitems", "lvl") => "Item level",
        ("runes", "complete") => "Enabled",
        ("runes", "rune1") => "First rune",
        ("runes", "rune2") => "Second rune",
        ("magicprefix" | "magicsuffix" | "automagic", "mod1code") => "Property",
        ("magicprefix" | "magicsuffix" | "automagic", "mod1min") => "Min value",
        ("magicprefix" | "magicsuffix" | "automagic", "mod1max") => "Max value",
        ("treasureclassex", "nodrop") => "No-drop weight",
        ("treasureclassex", "picks") => "Drop picks",
        ("cubemain", "numinputs") => "Input count",
        _ => key,
    }
}

/// Every column belongs to exactly one form section, including custom columns.
pub fn group(key: &str) -> &'static str {
    let k = key.to_ascii_lowercase();
    if k.contains("sock") || k.starts_with("gem") {
        "Sockets"
    } else if k.contains("req") || k == "lvl req" {
        "Requirements"
    } else if k.contains("dam") || k.ends_with("ac") || k == "speed" || k == "rangeadder" {
        "Damage"
    } else if [
        "code",
        "name",
        "namestr",
        "index",
        "type",
        "type2",
        "level",
        "lvl",
        "version",
        "enabled",
        "spawnable",
        "rarity",
    ]
    .contains(&k.as_str())
    {
        "Basics"
    } else {
        "Other"
    }
}

//! The quest log panel: a `files/ui/quest_log.toml` made from the quests of a
//! project. It replaces the game's quest log panel (`replace = "quest_log"`,
//! so it opens and closes with the same key) and binds to the UI variables
//! the quest runtime publishes to each player (`var.quests/q<id>` and
//! `var.quests/q<id>o<n>`, see `d2_sim::rules::data_mod::quests::variable_name`).
//! A quest the player has not begun has no variable yet, so its row shows
//! dashes.

use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use anyhow::{bail, Context, Result};
use d2_sim::rules::data_mod::quests::{variable_name, MAX_VARIABLES};

use crate::kit::{objective_text, Names, QuestEntry};

/// Where the panel is written, relative to the project.
pub const PANEL_FILE: &str = "files/ui/quest_log.toml";
/// The first line of a file this module wrote. Only such a file is replaced.
const MARK: &str =
    "# Written by Mod Studio's Quests page. Writing the panel again replaces this file.";

const LEFT: u16 = 140;
const TOP: u16 = 70;
const ROW: u16 = 20;
const BOTTOM: u16 = 690;
const MAX_ELEMENTS: usize = 256;

/// The panel and how many quests did not fit in it.
pub struct Made {
    pub text: String,
    pub left_out: usize,
}

/// How many panel variables the quests of the project may use up in the
/// worst case (all begun), and the most the runtime publishes.
pub fn variable_use(quests: &[QuestEntry]) -> (usize, usize) {
    let all = quests.iter().map(|q| 1 + q.objectives.len()).sum();
    (all, MAX_VARIABLES)
}

fn quoted(s: &str) -> String {
    toml::Value::String(s.to_owned()).to_string()
}

#[allow(clippy::too_many_arguments)]
fn element(out: &mut String, id: &str, kind: &str, x: u16, y: u16, w: u16, h: u16, text: &str) {
    let _ = write!(
        out,
        "\n[[elements]]\nid = {}\nkind = \"{kind}\"\nx = {x}\ny = {y}\nwidth = {w}\nheight = {h}\n",
        quoted(id)
    );
    if kind == "text" {
        let _ = writeln!(out, "color = 255\ntext = {}", quoted(text));
    } else {
        out.push_str("color = 255\n");
    }
}

/// The panel for `quests`, as many as fit in the screen and in a panel's
/// element budget; the rest are counted in `left_out`.
pub fn make(quests: &[QuestEntry], names: &Names) -> Made {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "{MARK}\nversion = 1\nname = \"mod_quest_log\"\nreplace = \"quest_log\""
    );
    element(&mut out, "title", "text", LEFT, 40, 400, 20, "Quests");
    let mut elements = 1;
    let mut y = TOP;
    let mut shown = 0;
    for q in quests {
        let rows = 1 + q.objectives.len();
        let need = 2 * rows;
        let height = u16::try_from(rows).unwrap_or(u16::MAX).saturating_mul(ROW);
        if y.saturating_add(height) > BOTTOM || elements + need > MAX_ELEMENTS {
            break;
        }
        shown += 1;
        elements += need;
        // The state: the bar is empty before the quest, half open, full done.
        let id = format!("q{}", q.id);
        element(
            &mut out,
            &format!("{id}_mark"),
            "bar",
            LEFT,
            y + 4,
            60,
            12,
            "",
        );
        let _ = write!(
            out,
            "[elements.bind]\nsource = \"var.{}\"\nminimum = 0\nmaximum = 2\n",
            variable_name(q.id, 0)
        );
        element(
            &mut out,
            &format!("{id}_name"),
            "text",
            LEFT + 70,
            y,
            700,
            ROW,
            &q.name,
        );
        y += ROW;
        for (k, o) in q.objectives.iter().enumerate() {
            let slot = u8::try_from(k + 1).unwrap_or(u8::MAX);
            element(
                &mut out,
                &format!("{id}_o{k}"),
                "text",
                LEFT + 90,
                y,
                600,
                ROW,
                &objective_text(names, o),
            );
            element(
                &mut out,
                &format!("{id}_o{k}_n"),
                "text",
                LEFT + 700,
                y,
                120,
                ROW,
                "",
            );
            let _ = write!(
                out,
                "[elements.bind]\nsource = \"var.{}\"\nminimum = 0\nmaximum = {}\nformat = \"range\"\n",
                variable_name(q.id, slot),
                o.target().max(1)
            );
            y += ROW;
        }
    }
    Made {
        text: out,
        left_out: quests.len() - shown,
    }
}

/// Writes the panel into the project. Refuses to replace a file this module
/// did not write, and a panel the engine's reader would refuse.
pub fn write(root: &Path, quests: &[QuestEntry], names: &Names) -> Result<Made> {
    let made = make(quests, names);
    d2_modkit::ui::Panel::parse(&made.text).map_err(anyhow::Error::msg)?;
    let path = root.join(PANEL_FILE);
    if let Ok(old) = fs::read_to_string(&path) {
        if !old.starts_with(MARK) {
            bail!("{PANEL_FILE} exists and was not written by the Quests page; rename it or edit it in the panel layout editor");
        }
    }
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    }
    fs::write(&path, &made.text).with_context(|| format!("writing {}", path.display()))?;
    Ok(made)
}

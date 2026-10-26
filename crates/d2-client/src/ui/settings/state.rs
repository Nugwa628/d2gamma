// D2RUST's own: the settings menu's state and input rules.
//! What the settings menu shows and how input changes it, with no drawing:
//! the open category (or the search results), the selected and hovered
//! row, the open drop-down, the search text and the settings being edited.
//! The Bevy side ([`super::menu`]) draws this state and turns clicks and
//! keys into the calls below; the Ctrl+O overlay uses the same state with
//! its tabs as the categories.
//!
//! Rules:
//! - Every change applies at once: [`MenuState::take_changed`] hands the
//!   new settings to the host, which saves and applies them.
//! - Up / Down move the selection over the rows that can be changed;
//!   Left / Right change the selected row's value (a slider by one step, a
//!   switch flips, a choice to the next value that can be picked); Enter
//!   flips a switch, opens a drop-down or runs an action; Tab and
//!   Shift+Tab (or Page Down / Up) go to the next or previous category.
//! - Typing a letter starts a search: the rows of every category whose
//!   words hold the text. Esc clears the search, then closes the menu.
//! - A drop-down shows its values; Up / Down change the value while it is
//!   open (the picture follows), Enter or Esc closes it.

use crate::app::config::Settings;

use super::table::{self, Category, Facts, Kind, PRESETS, TABLE};

/// A key the menu takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuKey {
    Up,
    Down,
    Left,
    Right,
    Enter,
    Escape,
    /// Tab or Page Down.
    NextCategory,
    /// Shift+Tab or Page Up.
    PrevCategory,
    Backspace,
    Char(char),
}

/// What the pointer pressed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Hit {
    Category(Category),
    /// A row's label area.
    Row(usize),
    Toggle(usize),
    /// Value `k` of a segmented row.
    Segment(usize, i64),
    /// A slider's track at fraction `f` (0 left, 1 right).
    Slider(usize, f32),
    /// A choice row's drop-down box.
    Dropdown(usize),
    /// Value `k` in an open drop-down.
    Option(usize, i64),
    /// The Preset drop-down box.
    Preset,
    /// Preset `p` in its open list.
    PresetOption(usize),
    Search,
    Reset,
    Done,
    /// A row's action button.
    Action(usize),
    /// The panel itself, where nothing else is.
    Panel,
}

/// What the menu asks of its host.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuEvent {
    /// Done, or Esc with nothing left to close.
    Close,
    /// The Configure Controls row.
    ConfigureControls,
}

/// The open drop-down.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Hash)]
pub enum Open {
    #[default]
    None,
    Row(usize),
    /// The Preset list, with the highlighted preset.
    Preset(usize),
}

/// The menu's state.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MenuState {
    pub category: Category,
    pub selected: Option<usize>,
    pub hovered: Option<usize>,
    pub search: String,
    pub search_focus: bool,
    pub open: Open,
    pub settings: Settings,
    pub facts: Facts,
    /// The categories this menu offers (the overlay: its tabs).
    pub categories: Vec<Category>,
    changed: bool,
    events: Vec<MenuEvent>,
}

impl MenuState {
    /// A menu over `categories`, open on `first` (or the first one).
    pub fn new(categories: &[Category], first: Category, settings: Settings) -> Self {
        let mut m = MenuState {
            categories: categories.to_vec(),
            settings,
            ..MenuState::default()
        };
        m.go(if categories.contains(&first) {
            first
        } else {
            categories.first().copied().unwrap_or_default()
        });
        m
    }

    /// The full menu: every category.
    pub fn full(first: Category, settings: Settings) -> Self {
        Self::new(&Category::ALL, first, settings)
    }

    /// Takes the settings in force (another menu or a hotkey changed them).
    pub fn sync(&mut self, settings: Settings, facts: Facts) {
        self.settings = settings;
        self.facts = facts;
    }

    /// The rows shown: the search's results across the offered
    /// categories, or the open category's rows.
    pub fn rows(&self) -> Vec<usize> {
        if self.search.trim().is_empty() {
            table::rows_of(self.category).collect()
        } else {
            table::search(&self.search)
                .into_iter()
                .filter(|&i| self.categories.contains(&TABLE[i].category))
                .collect()
        }
    }

    /// Whether row `i` can be changed now.
    pub fn enabled(&self, i: usize) -> bool {
        table::enabled(&TABLE[i], &self.settings, &self.facts)
    }

    /// The row the info panel describes: the hovered one, else the
    /// selected one.
    pub fn info_row(&self) -> Option<usize> {
        self.hovered.or(self.selected)
    }

    /// The new settings, once per change.
    pub fn take_changed(&mut self) -> Option<Settings> {
        std::mem::take(&mut self.changed).then_some(self.settings)
    }

    pub fn take_events(&mut self) -> Vec<MenuEvent> {
        std::mem::take(&mut self.events)
    }

    /// Opens category `c` with its first row that can be changed selected.
    pub fn go(&mut self, c: Category) {
        self.category = c;
        self.search.clear();
        self.search_focus = false;
        self.open = Open::None;
        self.hovered = None;
        self.selected = self.rows().into_iter().find(|&i| self.enabled(i));
    }

    /// Sets row `i` to `v` when it can be changed and `v` can be picked.
    pub fn set(&mut self, i: usize, v: i64) {
        let e = &TABLE[i];
        if !self.enabled(i) {
            return;
        }
        if matches!(e.kind, Kind::Choice(_) | Kind::Segmented(_))
            && !table::value_enabled(e, v, &self.settings, &self.facts)
        {
            return;
        }
        let before = self.settings;
        e.store(&mut self.settings, v);
        if self.settings != before {
            self.changed = true;
        }
    }

    /// Moves row `i` one step in `dir`.
    pub fn step(&mut self, i: usize, dir: i64) {
        let v = table::stepped(&TABLE[i], &self.settings, &self.facts, dir);
        self.set(i, v);
    }

    /// Puts the open category back to its defaults (Reset).
    pub fn reset(&mut self) {
        let before = self.settings;
        table::reset(&mut self.settings, self.category);
        if self.settings != before {
            self.changed = true;
        }
    }

    /// Sets preset `p`.
    pub fn apply_preset(&mut self, p: usize) {
        if let Some(preset) = PRESETS.get(p) {
            let before = self.settings;
            preset.apply(&mut self.settings);
            if self.settings != before {
                self.changed = true;
            }
        }
    }

    /// The preset in force (`None`: Custom).
    pub fn preset(&self) -> Option<usize> {
        table::current_preset(&self.settings)
    }

    fn activate(&mut self, i: usize) {
        if !self.enabled(i) {
            return;
        }
        let e = &TABLE[i];
        match e.kind {
            Kind::Toggle => self.step(i, 1),
            Kind::Choice(_) => self.open = Open::Row(i),
            Kind::Action => {
                if e.action == Some(table::Action::ConfigureControls) {
                    self.events.push(MenuEvent::ConfigureControls);
                }
            }
            Kind::Slider { .. } | Kind::Segmented(_) => {}
        }
    }

    fn move_selection(&mut self, dir: i64) {
        let rows: Vec<usize> = self
            .rows()
            .into_iter()
            .filter(|&i| self.enabled(i))
            .collect();
        if rows.is_empty() {
            self.selected = None;
            return;
        }
        let at = self
            .selected
            .and_then(|s| rows.iter().position(|&r| r == s));
        let next = match at {
            None => 0,
            Some(p) => (p as i64 + dir).clamp(0, rows.len() as i64 - 1) as usize,
        };
        self.selected = Some(rows[next]);
        self.hovered = None;
    }

    fn next_category(&mut self, dir: i64) {
        let n = self.categories.len() as i64;
        if n == 0 {
            return;
        }
        let at = self
            .categories
            .iter()
            .position(|&c| c == self.category)
            .unwrap_or(0) as i64;
        self.go(self.categories[(at + dir).rem_euclid(n) as usize]);
    }

    /// A key press.
    pub fn key(&mut self, k: MenuKey) {
        if let Open::Preset(p) = self.open {
            match k {
                MenuKey::Up => self.open = Open::Preset(p.saturating_sub(1)),
                MenuKey::Down => self.open = Open::Preset((p + 1).min(PRESETS.len() - 1)),
                MenuKey::Enter => {
                    self.apply_preset(p);
                    self.open = Open::None;
                }
                MenuKey::Escape => self.open = Open::None,
                _ => {}
            }
            return;
        }
        if let Open::Row(i) = self.open {
            match k {
                MenuKey::Up | MenuKey::Left => self.step(i, -1),
                MenuKey::Down | MenuKey::Right => self.step(i, 1),
                MenuKey::Enter | MenuKey::Escape => self.open = Open::None,
                _ => {}
            }
            return;
        }
        if self.search_focus {
            match k {
                MenuKey::Char(c) if !c.is_control() => {
                    if self.search.chars().count() < 40 {
                        self.search.push(c);
                    }
                    self.after_search();
                    return;
                }
                MenuKey::Backspace => {
                    self.search.pop();
                    self.after_search();
                    return;
                }
                MenuKey::Escape => {
                    self.search_focus = false;
                    if !self.search.is_empty() {
                        let c = self.category;
                        self.go(c);
                    }
                    return;
                }
                MenuKey::Enter | MenuKey::Down => {
                    self.search_focus = false;
                    self.selected = None;
                    self.move_selection(1);
                    return;
                }
                _ => self.search_focus = false,
            }
        }
        match k {
            MenuKey::Up => self.move_selection(-1),
            MenuKey::Down => self.move_selection(1),
            MenuKey::Left => {
                if let Some(i) = self.selected {
                    self.step(i, -1);
                }
            }
            MenuKey::Right => {
                if let Some(i) = self.selected {
                    self.step(i, 1);
                }
            }
            MenuKey::Enter => {
                if let Some(i) = self.selected {
                    self.activate(i);
                }
            }
            MenuKey::Escape => {
                if self.search.is_empty() {
                    self.events.push(MenuEvent::Close);
                } else {
                    let c = self.category;
                    self.go(c);
                }
            }
            MenuKey::NextCategory => self.next_category(1),
            MenuKey::PrevCategory => self.next_category(-1),
            MenuKey::Char(c) if c.is_alphanumeric() => {
                self.search_focus = true;
                self.search.push(c);
                self.after_search();
            }
            MenuKey::Backspace | MenuKey::Char(_) => {}
        }
    }

    fn after_search(&mut self) {
        self.open = Open::None;
        self.hovered = None;
        let rows = self.rows();
        if self.selected.is_none_or(|s| !rows.contains(&s)) {
            self.selected = rows.into_iter().find(|&i| self.enabled(i));
        }
    }

    /// The pointer moved over row `i` (`None`: over no row).
    pub fn hover(&mut self, i: Option<usize>) {
        self.hovered = i;
    }

    /// A press of the pointer on `hit`.
    pub fn press(&mut self, hit: Hit) {
        // A press anywhere but in the open list closes it.
        let keep_open = matches!(
            (hit, self.open),
            (Hit::Option(..), Open::Row(_)) | (Hit::PresetOption(_), Open::Preset(_))
        );
        let was_open = self.open;
        if !keep_open {
            self.open = Open::None;
        }
        if hit != Hit::Search {
            self.search_focus = false;
        }
        let select = |m: &mut Self, i: usize| {
            if m.enabled(i) {
                m.selected = Some(i);
            }
        };
        match hit {
            Hit::Category(c) => self.go(c),
            Hit::Row(i) => select(self, i),
            Hit::Toggle(i) => {
                select(self, i);
                self.step(i, 1);
            }
            Hit::Segment(i, k) => {
                select(self, i);
                self.set(i, k);
            }
            Hit::Slider(i, f) => {
                select(self, i);
                self.drag(i, f);
            }
            Hit::Dropdown(i) => {
                select(self, i);
                if was_open != Open::Row(i) && self.enabled(i) {
                    self.open = Open::Row(i);
                }
            }
            Hit::Option(i, k) => {
                self.set(i, k);
                self.open = Open::None;
            }
            Hit::Preset => {
                if !matches!(was_open, Open::Preset(_)) {
                    self.open = Open::Preset(self.preset().unwrap_or(0));
                }
            }
            Hit::PresetOption(p) => {
                self.apply_preset(p);
                self.open = Open::None;
            }
            Hit::Search => self.search_focus = true,
            Hit::Reset => self.reset(),
            Hit::Done => self.events.push(MenuEvent::Close),
            Hit::Action(i) => {
                select(self, i);
                self.activate(i);
            }
            Hit::Panel => {}
        }
    }

    /// A slider of row `i` held at fraction `f` of its track.
    pub fn drag(&mut self, i: usize, f: f32) {
        if let Kind::Slider { min, max, .. } = TABLE[i].kind {
            let v = min + ((max - min) as f32 * f.clamp(0.0, 1.0)).round() as i64;
            self.set(i, v);
        }
    }

    /// Where row `i`'s slider knob sits on its track, 0 to 1.
    pub fn slider_fraction(&self, i: usize) -> f32 {
        match TABLE[i].kind {
            Kind::Slider { min, max, .. } if max > min => {
                (TABLE[i].value(&self.settings) - min) as f32 / (max - min) as f32
            }
            _ => 0.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::settings::table::entry;

    fn idx(id: &str) -> usize {
        TABLE.iter().position(|e| e.id == id).expect(id)
    }

    fn menu() -> MenuState {
        MenuState::full(Category::Picture, Settings::default())
    }

    #[test]
    fn opens_on_the_first_row_of_its_category() {
        let m = menu();
        assert_eq!(m.selected, Some(idx("video.scale_mode")));
        assert!(m
            .rows()
            .iter()
            .all(|&i| TABLE[i].category == Category::Picture));
    }

    #[test]
    fn arrows_move_and_change_and_every_change_is_handed_over_once() {
        let mut m = menu();
        for _ in 0..3 {
            m.key(MenuKey::Down);
        }
        assert_eq!(m.selected, Some(idx("video.gamma")));
        m.key(MenuKey::Right);
        assert_eq!(m.settings.gamma, 165);
        assert_eq!(m.take_changed().map(|s| s.gamma), Some(165));
        assert_eq!(m.take_changed(), None);
        // At the end of the range nothing changes.
        m.settings.gamma = 255;
        m.key(MenuKey::Right);
        assert_eq!(m.take_changed(), None);
    }

    #[test]
    fn rows_that_cant_change_are_skipped() {
        let mut m = MenuState::full(Category::Display, Settings::default());
        m.settings.window_mode = crate::app::config::WindowMode::Fullscreen;
        m.selected = Some(idx("video.window_mode"));
        m.key(MenuKey::Down);
        assert_eq!(
            m.selected,
            Some(idx("video.menu_sides")),
            "Window Size is greyed"
        );
        m.press(Hit::Row(idx("video.window_size")));
        assert_eq!(m.selected, Some(idx("video.menu_sides")));
    }

    #[test]
    fn tab_walks_the_categories_and_wraps() {
        let mut m = MenuState::full(Category::Controls, Settings::default());
        m.key(MenuKey::NextCategory);
        assert_eq!(m.category, Category::Display);
        m.key(MenuKey::PrevCategory);
        assert_eq!(m.category, Category::Controls);
        let mut o = MenuState::new(&Category::OVERLAY, Category::Picture, Settings::default());
        o.key(MenuKey::NextCategory);
        assert_eq!(o.category, Category::Display);
        o.key(MenuKey::NextCategory);
        o.key(MenuKey::NextCategory);
        assert_eq!(o.category, Category::Picture);
    }

    #[test]
    fn typing_searches_every_category_and_esc_clears_then_closes() {
        let mut m = menu();
        for c in "vsync".chars() {
            m.key(MenuKey::Char(c));
        }
        // Frame Limit too: its description speaks of VSync.
        assert_eq!(m.rows(), vec![idx("video.vsync"), idx("video.frame_limit")]);
        assert_eq!(m.selected, Some(idx("video.vsync")));
        m.key(MenuKey::Escape);
        assert!(m.search.is_empty());
        assert_eq!(m.category, Category::Picture);
        m.key(MenuKey::Escape);
        assert_eq!(m.take_events(), vec![MenuEvent::Close]);
    }

    #[test]
    fn the_overlay_searches_only_its_tabs() {
        let mut o = MenuState::new(&Category::OVERLAY, Category::Picture, Settings::default());
        o.search_focus = true;
        for c in "music".chars() {
            o.key(MenuKey::Char(c));
        }
        assert!(o.rows().is_empty());
    }

    #[test]
    fn a_drop_down_opens_changes_live_and_closes() {
        let mut m = MenuState::full(Category::Display, Settings::default());
        let fl = idx("video.frame_limit");
        m.facts.refresh_hz = Some(144);
        m.press(Hit::Dropdown(fl));
        assert_eq!(m.open, Open::Row(fl));
        m.key(MenuKey::Down);
        assert_eq!(m.settings.frame_limit, 30);
        m.key(MenuKey::Enter);
        assert_eq!(m.open, Open::None);
        m.press(Hit::Dropdown(fl));
        m.press(Hit::Option(fl, 4));
        assert_eq!((m.settings.frame_limit, m.open), (144, Open::None));
        // A greyed value is refused (VSync on at 144 Hz: 165 and up).
        m.press(Hit::Dropdown(fl));
        m.press(Hit::Option(fl, 5));
        assert_eq!(m.settings.frame_limit, 144);
        // A click elsewhere closes it.
        m.press(Hit::Dropdown(fl));
        m.press(Hit::Panel);
        assert_eq!(m.open, Open::None);
    }

    #[test]
    fn clicks_flip_toggles_pick_segments_and_drag_sliders() {
        let mut m = menu();
        let b = idx("video.blended_shadows");
        m.press(Hit::Toggle(b));
        assert_eq!(m.settings.blended_shadows, 0);
        m.press(Hit::Segment(idx("video.scale_mode"), 1));
        assert_eq!(
            m.settings.scale_mode,
            crate::app::config::ScaleMode::Integer
        );
        let g = idx("video.gamma");
        m.press(Hit::Slider(g, 1.0));
        assert_eq!(m.settings.gamma, 255);
        m.drag(g, 0.5);
        assert_eq!(m.settings.gamma, 155);
        assert!((m.slider_fraction(g) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn reset_and_presets_change_the_settings() {
        let mut m = menu();
        m.press(Hit::Slider(idx("video.gamma"), 1.0));
        m.take_changed();
        m.press(Hit::Reset);
        assert_eq!(m.settings.gamma, 155);
        assert!(m.take_changed().is_some());
        m.press(Hit::Preset);
        assert_eq!(m.open, Open::Preset(0));
        m.key(MenuKey::Down);
        m.key(MenuKey::Enter);
        assert_eq!(m.preset(), Some(1));
        assert_eq!(
            m.settings.scale_mode,
            crate::app::config::ScaleMode::Integer
        );
    }

    /// The label names the preset picked until a setting changes: then it
    /// is Custom. The list ends at Performance.
    #[test]
    fn the_picked_preset_is_named_until_a_setting_changes() {
        let mut m = menu();
        assert_eq!(m.preset(), Some(0));
        m.press(Hit::Preset);
        m.key(MenuKey::Down);
        m.key(MenuKey::Down);
        m.key(MenuKey::Down);
        assert_eq!(m.open, Open::Preset(PRESETS.len() - 1), "the last");
        m.key(MenuKey::Enter);
        assert_eq!(PRESETS[m.preset().unwrap()].name, "Performance");
        m.press(Hit::Slider(idx("video.gamma"), 1.0));
        assert_eq!(m.preset(), None, "Custom");
        m.apply_preset(0);
        assert_eq!(m.preset(), Some(0));
    }

    #[test]
    fn configure_controls_is_asked_of_the_host() {
        let mut m = MenuState::full(Category::Controls, Settings::default());
        let c = idx("controls.configure");
        assert_eq!(m.selected, Some(c));
        m.key(MenuKey::Enter);
        assert_eq!(m.take_events(), vec![MenuEvent::ConfigureControls]);
        m.press(Hit::Action(c));
        assert_eq!(m.take_events(), vec![MenuEvent::ConfigureControls]);
        assert!(entry("controls.configure").is_some());
    }

    #[test]
    fn the_info_panel_follows_the_pointer_then_the_selection() {
        let mut m = menu();
        assert_eq!(m.info_row(), m.selected);
        m.hover(Some(idx("video.contrast")));
        assert_eq!(m.info_row(), Some(idx("video.contrast")));
        m.hover(None);
        assert_eq!(m.info_row(), m.selected);
    }
}

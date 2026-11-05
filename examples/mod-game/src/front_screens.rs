//! The example's own character screens, for the front end's hand-back
//! (`d2_client::mods::front_hand_back`): two screens with no art that pick
//! a character or create one through the [`FrontStart`] the front end gives
//! them, so the game starts with it.
//!
//! - Character select: Enter starts the game with the first saved
//!   character, N goes to character create, Esc leaves.
//! - Character create: Enter creates [`NEW_NAME`] (a paladin) and starts
//!   the game; it stays where it is when the engine refuses the name.
//!
//! They stand in for what a real mod game draws on the same two seams.

use d2_client::mods::front_hand_back::FrontStart;
use d2_client::mods::ui_hooks::FrontScreenFactory;
use d2_client::mods::ClientMods;
use d2_client::ui::front_end::control::vk;
use d2_client::ui::front_end::flow::Trigger;
use d2_client::ui::front_end::screens::create::{Class, NewCharacter};
use d2_client::ui::front_end::{FrontCtx, Screen, CHAR_CREATE, CHAR_SELECT};

/// The name the example's create screen gives its new character.
pub const NEW_NAME: &str = "Newcomer";

/// The virtual key of N.
const KEY_N: u16 = 0x4E;

/// The example's character select.
struct PickFirst(FrontStart);

impl Screen for PickFirst {
    fn key(&mut self, ctx: &mut FrontCtx, key: u16) -> Option<Trigger> {
        match key {
            vk::ENTER => {
                let first = self.0.characters().into_iter().next()?;
                Some(self.0.choose_entry(ctx, first))
            }
            KEY_N => Some(Trigger::CreateNew),
            vk::ESC => Some(Trigger::Exit),
            _ => None,
        }
    }
}

/// The example's character create.
struct CreateNewcomer(FrontStart);

impl Screen for CreateNewcomer {
    fn key(&mut self, _ctx: &mut FrontCtx, key: u16) -> Option<Trigger> {
        match key {
            vk::ENTER => self
                .0
                .create(NewCharacter {
                    name: NEW_NAME.into(),
                    class: Class::Paladin,
                    hardcore: false,
                    expansion: true,
                })
                .ok(),
            vk::ESC => Some(Trigger::Exit),
            _ => None,
        }
    }
}

/// What the factories make without the game start (the front end always
/// has one): a screen that hands nothing back.
struct Blank;

impl Screen for Blank {}

struct MakePickFirst;

impl FrontScreenFactory for MakePickFirst {
    fn make(&self) -> Box<dyn Screen> {
        Box::new(Blank)
    }

    fn make_with(&self, start: &FrontStart) -> Box<dyn Screen> {
        Box::new(PickFirst(start.clone()))
    }
}

struct MakeCreateNewcomer;

impl FrontScreenFactory for MakeCreateNewcomer {
    fn make(&self) -> Box<dyn Screen> {
        Box::new(Blank)
    }

    fn make_with(&self, start: &FrontStart) -> Box<dyn Screen> {
        Box::new(CreateNewcomer(start.clone()))
    }
}

/// The client hooks that put the example's character screens in place of
/// the engine's.
pub fn hand_back_mods() -> ClientMods {
    ClientMods::new()
        .with_front_screen(CHAR_SELECT, MakePickFirst)
        .with_front_screen(CHAR_CREATE, MakeCreateNewcomer)
}

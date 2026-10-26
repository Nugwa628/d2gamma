//! The example mod game's binary: brands itself as its own game, opens the
//! player's files with its own layers and builds a game under its own
//! rules with its own save folder (`docs/mods/mod-game.md`). Headless by
//! default (joins a new character and runs a few frames); `--window` plays
//! it in a window, as D2Gamma's exe plays its own game: the front end
//! first, or straight into the `--name` character.

use d2_client::app::{play, single_player};
use mod_game_example::{
    builder, front_end_config, identity, rules_name, run_frames, saves, start_and_join,
    window_config, Options, StepClock,
};

fn main() -> anyhow::Result<()> {
    // Before anything reads the game's name or its Documents folder.
    d2_client::identity::install(identity())
        .map_err(|_| anyhow::anyhow!("the identity was set twice"))?;
    let options = Options::parse(std::env::args().skip(1))?;
    let (user, _) = d2_client::user_dir::init();
    let data = single_player::GameData::open(&options.source)?;
    let saves_dir = options.saves_dir();
    if let (true, None) = (options.window, &options.name) {
        // The front end: the example's main menu, its characters.
        play::run_front_end(front_end_config(
            data,
            options.game_dir(),
            saves_dir.clone(),
        ))?;
        println!(
            "{}: window closed; saves in {}, settings in {}",
            d2_client::identity::current().name,
            saves_dir.display(),
            user.display()
        );
        return Ok(());
    }
    if let (true, Some(name)) = (options.window, &options.name) {
        let saves = saves(saves_dir.clone());
        let path = saves.file(name);
        // The character's save in the example's folder, else a new one.
        let character = if path.is_file() {
            saves.load(&data, &path, 0)?
        } else {
            single_player::new_character("sorceress", name)?
        };
        let end = play::run(window_config(
            data,
            saves,
            character,
            Some(path),
            options.frames,
        ))?;
        println!(
            "{}: window closed ({:?}); saves in {}, settings in {}",
            d2_client::identity::current().name,
            end.exit,
            saves_dir.display(),
            user.display()
        );
        return Ok(());
    }
    let clock = StepClock::default();
    let (mut link, _) = start_and_join(builder(data, saves_dir.clone()), &clock)?;
    let ticks = run_frames(&mut link, &clock, 25)?;
    println!(
        "{}: {ticks} ticks, saves in {}",
        rules_name(&mut link)?.unwrap_or_else(|| "D2RUST".into()),
        saves_dir.display()
    );
    Ok(())
}

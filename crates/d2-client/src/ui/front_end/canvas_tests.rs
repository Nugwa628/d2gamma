use super::*;
use crate::rules::camera::FrameSize;

fn front(expansion: bool) -> FrontEnd {
    let mut registry = Registry::default();
    screens::register_all(&mut registry);
    screens::credits::register_with(&mut registry, Box::new(|_| None));
    FrontEnd::new(expansion, Box::new(false), registry)
}

#[test]
fn matrix_controls_use_the_same_canvas_for_draws_and_clicks() {
    for frame in crate::ui::test_frames::FRAMES {
        for expansion in [false, true] {
            let mut f = front(expansion);
            f.set_canvas(frame);
            for screen in [
                MAIN_MENU,
                CHAR_SELECT,
                CHAR_CREATE,
                CONTROLS,
                CREDITS,
                DIFFICULTY,
            ] {
                f.goto(screen);
                for e in f.ui_elements() {
                    assert!(
                        e.rect.x >= 0 && e.rect.right() <= i64::from(frame.width),
                        "{screen:?} {e:?} {frame:?}"
                    );
                    assert!(
                        e.rect.y >= 0 && e.rect.bottom() <= i64::from(frame.height),
                        "{screen:?} {e:?} {frame:?}"
                    );
                    assert!(e.rect.contains(e.at));
                }
            }
            f.goto(MAIN_MENU);
            let exit = f
                .ui_elements()
                .into_iter()
                .find(|e| e.id == "menu.exit")
                .unwrap();
            let draws = f.draw();
            assert!(draws
                .iter()
                .any(|item| matches!(item,DrawItem::Text{at,label:Some(_),..}
                if at.x==exit.rect.x && at.y==exit.rect.bottom() as i32)));
            f.input(FrontInput::Move(exit.at));
            f.input(FrontInput::Down(exit.at));
            f.tick();
            f.input(FrontInput::Up(exit.at));
            f.tick();
            assert_eq!(f.outcome(), Some(Outcome::Exit));
        }
    }
}

#[test]
fn live_canvas_round_trip_preserves_controls_and_fixed_draws() {
    let mut f = front(true);
    f.goto(MAIN_MENU);
    let baseline = f.draw();
    let controls = f.controls().to_vec();
    for frame in [
        FrameSize {
            width: 960,
            height: 540,
        },
        FrameSize {
            width: 801,
            height: 541,
        },
        FrameSize {
            width: 800,
            height: 1600,
        },
        FrameSize::D2RUST,
    ] {
        f.set_canvas(frame);
        assert_eq!(f.controls(), controls);
    }
    assert_eq!(f.draw(), baseline);
}

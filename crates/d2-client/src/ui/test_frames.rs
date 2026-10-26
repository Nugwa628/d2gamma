//! Frames of many shapes for the layout tests: the HUD, panels and front
//! end must place their art and hit regions inside any frame, the
//! 1280 × 720 view among them.

use crate::rules::camera::FrameSize;

const fn f(width: i32, height: i32) -> FrameSize {
    FrameSize { width, height }
}

/// Wide, tall, odd and minimum-size frames.
pub const FRAMES: [FrameSize; 10] = [
    f(960, 540),
    f(1280, 720),
    f(960, 600),
    f(1280, 540),
    f(1720, 720),
    f(1365, 767),
    f(1919, 1079),
    f(800, 1600),
    f(1920, 540),
    f(801, 541),
];

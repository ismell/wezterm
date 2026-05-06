// clippy hates bitflags
#![allow(clippy::suspicious_arithmetic_impl, clippy::redundant_field_names)]

use super::VisibleRowIndex;
#[cfg(feature = "use_serde")]
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};
use wezterm_dynamic::{FromDynamic, ToDynamic};

pub use termwiz::input::{KeyCode, Modifiers as KeyModifiers};

#[cfg_attr(feature = "use_serde", derive(Deserialize, Serialize))]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Ord, PartialOrd, Hash, FromDynamic, ToDynamic)]
pub enum MouseButton {
    Left,
    Middle,
    Right,
    WheelUp(usize),
    WheelDown(usize),
    WheelLeft(usize),
    WheelRight(usize),
    None,
}

#[cfg_attr(feature = "use_serde", derive(Deserialize, Serialize))]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum MouseEventKind {
    Press,
    Release,
    Move,
}

#[cfg_attr(feature = "use_serde", derive(Deserialize, Serialize))]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct MouseEvent {
    pub kind: MouseEventKind,
    pub x: usize,
    pub y: VisibleRowIndex,
    pub x_pixel_offset: isize,
    pub y_pixel_offset: isize,
    pub button: MouseButton,
    pub modifiers: KeyModifiers,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct ClickPosition {
    pub column: usize,
    pub row: i64,
    pub x_pixel_offset: isize,
    pub y_pixel_offset: isize,
}

/// This is a little helper that keeps track of the "click streak",
/// which is the number of successive clicks of the same mouse button
/// within the `CLICK_INTERVAL`.  The streak is reset to 1 each time
/// the mouse button differs from the last click, or when the elapsed
/// time exceeds `CLICK_INTERVAL`, or when the cursor position
/// changes to a different character cell.
#[derive(Debug, Clone)]
pub struct LastMouseClick {
    pub button: MouseButton,
    pub position: ClickPosition,
    time: Instant,
    pub streak: usize,
}

/// The multi-click interval, measured in milliseconds
const CLICK_INTERVAL: u64 = 500;

impl LastMouseClick {
    pub fn new(button: MouseButton, position: ClickPosition) -> Self {
        Self {
            button,
            position,
            time: Instant::now(),
            streak: 1,
        }
    }

    pub fn add(&self, button: MouseButton, position: ClickPosition) -> Self {
        let now = Instant::now();
        let dur = now.duration_since(self.time);
        let streak = if button == self.button
            && position.column.abs_diff(self.position.column) <= 1
            && position.row.abs_diff(self.position.row) <= 1
            && dur <= Duration::from_millis(CLICK_INTERVAL)
        {
            self.streak + 1
        } else {
            1
        };
        Self {
            button,
            position,
            time: now,
            streak,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_forgiving_double_click() {
        let click = LastMouseClick::new(
            MouseButton::Left,
            ClickPosition {
                column: 10,
                row: 5,
                x_pixel_offset: 0,
                y_pixel_offset: 0,
            },
        );

        // 1 cell away should count as a streak
        let streak = click.add(
            MouseButton::Left,
            ClickPosition {
                column: 11,
                row: 6,
                x_pixel_offset: 0,
                y_pixel_offset: 0,
            },
        );
        assert_eq!(streak.streak, 2);

        // 2 cells away should reset the streak
        let reset = click.add(
            MouseButton::Left,
            ClickPosition {
                column: 12,
                row: 5,
                x_pixel_offset: 0,
                y_pixel_offset: 0,
            },
        );
        assert_eq!(reset.streak, 1);
    }
}

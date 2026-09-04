//! Adaptive scheduling screen stream для T3.

use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamVisibility {
    Hidden,
    Visible,
    Selected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StreamSchedule {
    pub fps: u32,
    pub max_width: u32,
    pub mode: StreamVisibility,
}

impl StreamSchedule {
    pub fn interval(self) -> Option<Duration> {
        (self.fps > 0).then(|| Duration::from_millis(1_000 / u64::from(self.fps)))
    }
}

/// Ограничивает подсказки Teacher безопасными пределами Agent.
pub fn negotiate_schedule(
    mode: StreamVisibility,
    requested_fps: u32,
    requested_width: u32,
) -> StreamSchedule {
    match mode {
        StreamVisibility::Hidden => StreamSchedule {
            fps: 0,
            max_width: 0,
            mode,
        },
        StreamVisibility::Visible => StreamSchedule {
            fps: requested_fps.clamp(1, 2),
            max_width: requested_width.clamp(320, 640),
            mode,
        },
        StreamVisibility::Selected => StreamSchedule {
            fps: requested_fps.clamp(1, 15),
            max_width: requested_width.clamp(640, 3_840),
            mode,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hidden_subscription_stops_frames() {
        let schedule = negotiate_schedule(StreamVisibility::Hidden, 15, 1920);
        assert_eq!(schedule.fps, 0);
        assert_eq!(schedule.interval(), None);
    }

    #[test]
    fn visible_and_selected_are_bounded_differently() {
        let thumbnail = negotiate_schedule(StreamVisibility::Visible, 30, 1920);
        let selected = negotiate_schedule(StreamVisibility::Selected, 30, 5000);
        assert_eq!(thumbnail.fps, 2);
        assert_eq!(thumbnail.max_width, 640);
        assert_eq!(selected.fps, 15);
        assert_eq!(selected.max_width, 3840);
    }
}

//! Adaptive scheduling screen stream для T3.

use std::collections::VecDeque;
use std::time::Duration;

pub const STREAM_SUBSCRIPTION_TIMEOUT: Duration = Duration::from_secs(15);

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActiveSubscription {
    pub schedule: StreamSchedule,
    pub last_seen_unix_ms: i64,
}

impl ActiveSubscription {
    pub fn new(schedule: StreamSchedule, now_unix_ms: i64) -> Self {
        Self {
            schedule,
            last_seen_unix_ms: now_unix_ms,
        }
    }

    pub fn refresh(&mut self, now_unix_ms: i64) {
        self.last_seen_unix_ms = now_unix_ms;
    }

    pub fn is_expired(self, now_unix_ms: i64) -> bool {
        let timeout = i64::try_from(STREAM_SUBSCRIPTION_TIMEOUT.as_millis()).unwrap_or(i64::MAX);
        now_unix_ms.saturating_sub(self.last_seen_unix_ms) > timeout
    }
}

impl StreamSchedule {
    pub fn interval(self) -> Option<Duration> {
        (self.fps > 0).then(|| Duration::from_millis(1_000 / u64::from(self.fps)))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameClock {
    schedule: StreamSchedule,
    next_due_unix_ms: i64,
    sequence: u32,
}

impl FrameClock {
    pub fn new(schedule: StreamSchedule, now_unix_ms: i64) -> Self {
        Self {
            schedule,
            next_due_unix_ms: now_unix_ms,
            sequence: 0,
        }
    }

    pub fn update(&mut self, schedule: StreamSchedule, now_unix_ms: i64) {
        self.schedule = schedule;
        self.next_due_unix_ms = now_unix_ms;
    }

    /// Возвращает sequence одного кадра, если он уже должен быть создан.
    /// После долгой паузы не пытается наверстать пропущенные кадры burst'ом.
    pub fn take_due(&mut self, now_unix_ms: i64) -> Option<u32> {
        let interval = self.schedule.interval()?;
        if now_unix_ms < self.next_due_unix_ms {
            return None;
        }
        let sequence = self.sequence;
        self.sequence = self.sequence.wrapping_add(1);
        self.next_due_unix_ms =
            now_unix_ms.saturating_add(i64::try_from(interval.as_millis()).unwrap_or(i64::MAX));
        Some(sequence)
    }

    pub fn schedule(self) -> StreamSchedule {
        self.schedule
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutboundPriority {
    Control,
    SelectedScreen,
    ThumbnailScreen,
}

/// Ограниченная логическая очередь: control отправляется раньше кадров, а
/// устаревшие screen frames удаляются при заполнении.
#[derive(Debug)]
pub struct PriorityQueue<T> {
    control: VecDeque<T>,
    selected: VecDeque<T>,
    thumbnails: VecDeque<T>,
    screen_capacity: usize,
}

impl<T> PriorityQueue<T> {
    pub fn new(screen_capacity: usize) -> Self {
        Self {
            control: VecDeque::new(),
            selected: VecDeque::new(),
            thumbnails: VecDeque::new(),
            screen_capacity: screen_capacity.max(1),
        }
    }

    pub fn push(&mut self, priority: OutboundPriority, value: T) {
        let queue = match priority {
            OutboundPriority::Control => {
                self.control.push_back(value);
                return;
            }
            OutboundPriority::SelectedScreen => &mut self.selected,
            OutboundPriority::ThumbnailScreen => &mut self.thumbnails,
        };
        if queue.len() >= self.screen_capacity {
            queue.pop_front();
        }
        queue.push_back(value);
    }

    pub fn pop(&mut self) -> Option<T> {
        self.control
            .pop_front()
            .or_else(|| self.selected.pop_front())
            .or_else(|| self.thumbnails.pop_front())
    }

    pub fn is_empty(&self) -> bool {
        self.control.is_empty() && self.selected.is_empty() && self.thumbnails.is_empty()
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

    #[test]
    fn control_overtakes_screen_frames() {
        let mut queue = PriorityQueue::new(2);
        queue.push(OutboundPriority::ThumbnailScreen, "thumbnail");
        queue.push(OutboundPriority::SelectedScreen, "selected");
        queue.push(OutboundPriority::Control, "heartbeat");
        assert_eq!(queue.pop(), Some("heartbeat"));
        assert_eq!(queue.pop(), Some("selected"));
        assert_eq!(queue.pop(), Some("thumbnail"));
    }

    #[test]
    fn screen_queue_drops_stale_frames() {
        let mut queue = PriorityQueue::new(1);
        queue.push(OutboundPriority::ThumbnailScreen, 1);
        queue.push(OutboundPriority::ThumbnailScreen, 2);
        assert_eq!(queue.pop(), Some(2));
        assert!(queue.is_empty());
    }

    #[test]
    fn subscription_expires_without_teacher_traffic() {
        let schedule = negotiate_schedule(StreamVisibility::Visible, 1, 640);
        let mut subscription = ActiveSubscription::new(schedule, 1_000);
        assert!(!subscription.is_expired(15_000));
        subscription.refresh(10_000);
        assert!(!subscription.is_expired(20_000));
        assert!(subscription.is_expired(26_000));
    }

    #[test]
    fn frame_clock_does_not_burst_after_pause() {
        let schedule = negotiate_schedule(StreamVisibility::Visible, 2, 640);
        let mut clock = FrameClock::new(schedule, 1_000);
        assert_eq!(clock.take_due(1_000), Some(0));
        assert_eq!(clock.take_due(1_499), None);
        assert_eq!(clock.take_due(10_000), Some(1));
        assert_eq!(clock.take_due(10_001), None);
    }

    #[test]
    fn hidden_clock_stops_and_selected_restarts_immediately() {
        let hidden = negotiate_schedule(StreamVisibility::Hidden, 1, 640);
        let selected = negotiate_schedule(StreamVisibility::Selected, 15, 1920);
        let mut clock = FrameClock::new(hidden, 1_000);
        assert_eq!(clock.take_due(2_000), None);
        clock.update(selected, 2_000);
        assert_eq!(clock.take_due(2_000), Some(0));
        assert_eq!(clock.schedule(), selected);
    }
}

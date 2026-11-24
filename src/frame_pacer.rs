//! Frame deadlines independent of input events and redraw requests.
use std::time::{Duration, Instant};
use winit::event_loop::ControlFlow;

#[derive(Default)]
pub struct FramePacer {
    last_frame: Option<Instant>,
}

impl FramePacer {
    pub fn frame_started(&mut self, now: Instant) {
        self.last_frame = Some(now);
    }

    /// Poll only when a frame is due. Early wakeups keep the same deadline.
    pub fn control_flow(&self, now: Instant, cap: u16) -> ControlFlow {
        if cap > 0 {
            if let Some(last) = self.last_frame {
                let deadline = last + Duration::from_secs_f64(1.0 / cap as f64);
                if now < deadline {
                    return ControlFlow::WaitUntil(deadline);
                }
            }
        }
        ControlFlow::Poll
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn regression_input_wakeups_do_not_request_early_frames_or_delay_the_deadline() {
        let mut pacer = FramePacer::default();
        let start = Instant::now();
        assert_eq!(pacer.control_flow(start, 30), ControlFlow::Poll);
        pacer.frame_started(start);
        let deadline = start + Duration::from_secs_f64(1.0 / 30.0);
        for ms in 0..34 {
            assert_eq!(pacer.control_flow(start + Duration::from_millis(ms), 30), ControlFlow::WaitUntil(deadline));
        }
        assert_eq!(pacer.control_flow(deadline, 30), ControlFlow::Poll);
    }

    #[test]
    fn regression_slow_frames_do_not_accumulate_catch_up_frames() {
        let mut pacer = FramePacer::default();
        let start = Instant::now();
        pacer.frame_started(start);
        let next = start + Duration::from_millis(100);
        assert_eq!(pacer.control_flow(next, 60), ControlFlow::Poll);
        pacer.frame_started(next);
        assert_eq!(pacer.control_flow(next, 60), ControlFlow::WaitUntil(next + Duration::from_secs_f64(1.0 / 60.0)));
    }

    #[test]
    fn regression_frame_limit_changes_apply_without_waiting_for_the_old_limit() {
        let mut pacer = FramePacer::default();
        let start = Instant::now();
        pacer.frame_started(start);
        let now = start + Duration::from_millis(20);
        assert!(matches!(pacer.control_flow(now, 30), ControlFlow::WaitUntil(_)));
        assert_eq!(pacer.control_flow(now, 60), ControlFlow::Poll);
        assert_eq!(pacer.control_flow(start, 0), ControlFlow::Poll);
    }
}

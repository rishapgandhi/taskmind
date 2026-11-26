//! Pure recorder state machine: Idle -> Recording <-> Paused -> Idle. No I/O, fully unit tested.

use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Idle,
    Recording,
    Paused,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Start,
    Pause,
    Resume,
    Stop,
    Toggle,
}

#[derive(Debug, thiserror::Error)]
#[error("cannot {action:?} while {from:?}")]
pub struct InvalidTransition {
    pub from: Status,
    pub action: Action,
}

#[derive(Debug)]
pub struct Machine {
    status: Status,
    started_at: Option<DateTime<Utc>>,
    accumulated: Duration,
    running_since: Option<Instant>,
}

impl Default for Machine {
    fn default() -> Self {
        Self {
            status: Status::Idle,
            started_at: None,
            accumulated: Duration::ZERO,
            running_since: None,
        }
    }
}

impl Machine {
    pub fn status(&self) -> Status {
        self.status
    }

    pub fn started_at(&self) -> Option<DateTime<Utc>> {
        self.started_at
    }

    /// Turn `Toggle` into the concrete action for the current state.
    pub fn resolve(&self, action: Action) -> Action {
        match (action, self.status) {
            (Action::Toggle, Status::Idle) => Action::Start,
            (Action::Toggle, _) => Action::Stop,
            (a, _) => a,
        }
    }

    /// Check an action is allowed without changing state.
    pub fn check(&self, action: Action) -> Result<Action, InvalidTransition> {
        let action = self.resolve(action);
        let ok = matches!(
            (action, self.status),
            (Action::Start, Status::Idle)
                | (Action::Pause, Status::Recording)
                | (Action::Resume, Status::Paused)
                | (Action::Stop, Status::Recording | Status::Paused)
        );
        if ok {
            Ok(action)
        } else {
            Err(InvalidTransition {
                from: self.status,
                action,
            })
        }
    }

    pub fn apply(&mut self, action: Action) -> Result<Action, InvalidTransition> {
        let action = self.check(action)?;
        match action {
            Action::Start => {
                *self = Self {
                    status: Status::Recording,
                    started_at: Some(Utc::now()),
                    accumulated: Duration::ZERO,
                    running_since: Some(Instant::now()),
                };
            }
            Action::Pause => {
                self.accumulated += self
                    .running_since
                    .take()
                    .map_or(Duration::ZERO, |t| t.elapsed());
                self.status = Status::Paused;
            }
            Action::Resume => {
                self.running_since = Some(Instant::now());
                self.status = Status::Recording;
            }
            Action::Stop | Action::Toggle => *self = Self::default(),
        }
        Ok(action)
    }

    pub fn elapsed(&self) -> Duration {
        self.accumulated + self.running_since.map_or(Duration::ZERO, |t| t.elapsed())
    }

    pub fn elapsed_ms(&self) -> u64 {
        u64::try_from(self.elapsed().as_millis()).unwrap_or(u64::MAX)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_cycle() {
        let mut m = Machine::default();
        m.apply(Action::Start).unwrap();
        assert_eq!(m.status(), Status::Recording);
        assert!(m.started_at().is_some());
        m.apply(Action::Pause).unwrap();
        assert_eq!(m.status(), Status::Paused);
        m.apply(Action::Resume).unwrap();
        m.apply(Action::Stop).unwrap();
        assert_eq!(m.status(), Status::Idle);
        assert_eq!(m.elapsed_ms(), 0);
        assert!(m.started_at().is_none());
    }

    #[test]
    fn toggle_starts_and_stops() {
        let mut m = Machine::default();
        assert_eq!(m.apply(Action::Toggle).unwrap(), Action::Start);
        m.apply(Action::Pause).unwrap();
        assert_eq!(m.apply(Action::Toggle).unwrap(), Action::Stop);
        assert_eq!(m.status(), Status::Idle);
    }

    #[test]
    fn invalid_transitions_are_rejected_without_changing_state() {
        let mut m = Machine::default();
        assert!(m.apply(Action::Stop).is_err());
        assert!(m.apply(Action::Pause).is_err());
        assert!(m.apply(Action::Resume).is_err());
        assert_eq!(m.status(), Status::Idle);
        m.apply(Action::Start).unwrap();
        assert!(m.check(Action::Start).is_err());
        assert!(m.apply(Action::Resume).is_err());
        assert_eq!(m.status(), Status::Recording);
    }

    #[test]
    fn paused_time_is_not_counted() {
        let mut m = Machine::default();
        m.apply(Action::Start).unwrap();
        std::thread::sleep(Duration::from_millis(30));
        m.apply(Action::Pause).unwrap();
        let at_pause = m.elapsed();
        std::thread::sleep(Duration::from_millis(30));
        assert_eq!(m.elapsed(), at_pause);
        assert!(at_pause >= Duration::from_millis(30));
    }
}

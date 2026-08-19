use crate::schema::LapSummary;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LapState {
    Waiting,
    InLap,
}

pub struct LapDetectionEngine {
    state: LapState,
    current_lap: u32,
    last_completed: u32,
}

impl Default for LapDetectionEngine {
    fn default() -> Self {
        Self {
            state: LapState::Waiting,
            current_lap: 0,
            last_completed: u32::MAX,
        }
    }
}

impl LapDetectionEngine {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn on_completed_laps(&mut self, completed_laps: u32) -> Option<LapBoundary> {
        if completed_laps > self.last_completed && self.last_completed != u32::MAX {
            let finished = self.last_completed + 1;
            self.current_lap = completed_laps + 1;
            self.last_completed = completed_laps;
            return Some(LapBoundary::Completed { lap_number: finished });
        }

        if self.last_completed == u32::MAX {
            self.last_completed = completed_laps;
            if completed_laps == 0 {
                self.current_lap = 1;
                self.state = LapState::InLap;
                return Some(LapBoundary::Started { lap_number: 1 });
            }
        }

        if self.state == LapState::Waiting && completed_laps == 0 {
            self.state = LapState::InLap;
            self.current_lap = 1;
            return Some(LapBoundary::Started { lap_number: 1 });
        }

        None
    }

    pub fn current_lap(&self) -> u32 {
        self.current_lap.max(1)
    }
}

#[derive(Debug, Clone)]
pub enum LapBoundary {
    Started { lap_number: u32 },
    Completed { lap_number: u32 },
}

pub fn is_valid_lap(summary: &LapSummary) -> bool {
    summary.valid && summary.lap_time_ms > 0
}

pub fn pick_best_lap(laps: &[LapSummary]) -> Option<u32> {
    laps.iter()
        .filter(|l| is_valid_lap(l))
        .min_by_key(|l| l.lap_time_ms)
        .map(|l| l.lap_number)
}

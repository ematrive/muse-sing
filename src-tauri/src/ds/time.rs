//! Tick ↔ millisecond conversion over a tempo map (OpenUtau.Core/Util/TimeAxis.cs), 480 ticks per quarter.

#[derive(Debug, Clone)]
struct Segment {
    tick: f64,
    ms: f64,
    ms_per_tick: f64,
}

#[derive(Debug, Clone)]
pub struct TimeAxis {
    segments: Vec<Segment>,
}

impl TimeAxis {
    /// `tempos`: (tick position, bpm), any order; an empty map means 120 bpm.
    pub fn new(tempos: &[(i64, f64)], ticks_per_quarter: i64) -> TimeAxis {
        let mut t: Vec<(i64, f64)> = tempos.iter().copied().filter(|(_, b)| *b > 0.0).collect();
        t.sort_by_key(|x| x.0);
        if t.first().is_none_or(|x| x.0 > 0) {
            let bpm = t.first().map(|x| x.1).unwrap_or(120.0);
            t.insert(0, (0, bpm));
        }
        let mut segments: Vec<Segment> = vec![];
        for (tick, bpm) in t {
            let ms_per_tick = 60000.0 / (bpm * ticks_per_quarter as f64);
            let ms = match segments.last() {
                Some(s) => s.ms + (tick as f64 - s.tick) * s.ms_per_tick,
                None => 0.0,
            };
            if let Some(s) = segments.last_mut()
                && s.tick == tick as f64
            {
                s.ms_per_tick = ms_per_tick;
                continue;
            }
            segments.push(Segment { tick: tick as f64, ms, ms_per_tick });
        }
        TimeAxis { segments }
    }

    pub fn tick_to_ms(&self, tick: f64) -> f64 {
        let s = self.segments.iter().rev().find(|s| s.tick <= tick).unwrap_or(&self.segments[0]);
        s.ms + s.ms_per_tick * (tick - s.tick)
    }

    pub fn ms_to_tick(&self, ms: f64) -> i64 {
        let s = self.segments.iter().rev().find(|s| s.ms <= ms).unwrap_or(&self.segments[0]);
        let t = s.tick + (ms - s.ms) / s.ms_per_tick;
        if t.is_finite() { t.round() as i64 } else { s.tick as i64 }
    }

    pub fn ticks_between_ms(&self, ms: f64, ms_end: f64) -> i64 {
        self.ms_to_tick(ms_end) - self.ms_to_tick(ms)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tempo_changes() {
        let a = TimeAxis::new(&[(0, 120.0), (960, 60.0)], 480);
        let close = |a: f64, b: f64| assert!((a - b).abs() < 1e-9, "{a} != {b}");
        close(a.tick_to_ms(480.0), 500.0);
        close(a.tick_to_ms(960.0), 1000.0);
        close(a.tick_to_ms(1440.0), 2000.0);
        assert_eq!(a.ms_to_tick(2000.0), 1440);
        assert_eq!(a.ms_to_tick(-500.0), -480);
        close(TimeAxis::new(&[], 480).tick_to_ms(480.0), 500.0);
    }
}

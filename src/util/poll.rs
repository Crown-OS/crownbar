//! Simple "every N ticks" gate. The bar's tick fires at 1 Hz, but most
//! widgets only need a fresh reading every few seconds — `PollGate::should_run`
//! returns `true` once per period so a widget's `update()` can early-return
//! between samples.

pub struct PollGate {
    period: u32,
    counter: u32,
}

impl PollGate {
    pub const fn new(period_ticks: u32) -> Self {
        Self {
            period: if period_ticks == 0 { 1 } else { period_ticks },
            counter: 0,
        }
    }

    pub fn should_run(&mut self) -> bool {
        let run = self.counter == 0;
        self.counter = (self.counter + 1) % self.period;
        run
    }
}

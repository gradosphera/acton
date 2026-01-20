use crate::Rule;
use std::collections::HashMap;
use std::time::Duration;

#[derive(Default, Debug, Clone)]
pub struct RuleStats {
    pub calls: u64,
    pub total: Duration,
}

#[derive(Default)]
pub struct Profiler {
    pub rules: HashMap<Rule, RuleStats>,
}

impl Profiler {
    pub fn record(&mut self, rule: Rule, elapsed: Duration) {
        let s = self.rules.entry(rule).or_default();
        s.calls += 1;
        s.total += elapsed;
    }
}

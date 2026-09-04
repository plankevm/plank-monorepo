use std::{fmt::Write, time::Duration};

const PERCENTILES: [usize; 5] = [5, 20, 50, 80, 95];

pub struct Stats {
    graph_count: usize,
    best_known_total_gas: u64,
    local_total_gas: u64,
    improvements: usize,
    gas_saved: u64,
    capped_searches: usize,
    deltas: Vec<i64>,
}

impl Stats {
    pub fn new(graph_count: usize) -> Self {
        Self {
            graph_count,
            best_known_total_gas: 0,
            local_total_gas: 0,
            improvements: 0,
            gas_saved: 0,
            capped_searches: 0,
            deltas: Vec::with_capacity(graph_count),
        }
    }

    pub fn record(&mut self, best_known_gas: u64, local_gas: u64, candidate_limit_reached: bool) {
        self.best_known_total_gas = self
            .best_known_total_gas
            .checked_add(best_known_gas)
            .expect("best-known total gas overflow");
        self.local_total_gas =
            self.local_total_gas.checked_add(local_gas).expect("local total gas overflow");
        let best_known = i64::try_from(best_known_gas).expect("best-known gas does not fit i64");
        let local = i64::try_from(local_gas).expect("local gas does not fit i64");
        let delta = best_known.checked_sub(local).expect("gas delta overflow");
        self.deltas.push(delta);
        if delta > 0 {
            self.improvements += 1;
            self.gas_saved = self
                .gas_saved
                .checked_add(u64::try_from(delta).expect("positive gas delta does not fit u64"))
                .expect("saved gas overflow");
        }
        self.capped_searches += usize::from(candidate_limit_reached);
    }

    pub fn has_improvements(&self) -> bool {
        self.improvements != 0
    }

    pub fn render(mut self, elapsed: Duration) -> String {
        assert_eq!(self.deltas.len(), self.graph_count);
        self.deltas.sort_unstable();
        let score = if self.local_total_gas == 0 {
            if self.best_known_total_gas == 0 { "100.00%".to_owned() } else { "∞%".to_owned() }
        } else {
            format!(
                "{:.2}%",
                self.best_known_total_gas as f64 / self.local_total_gas as f64 * 100.0
            )
        };
        let mut output = String::new();
        writeln!(output, "graphs: {}", self.graph_count).unwrap();
        writeln!(output, "best known total gas: {}", self.best_known_total_gas).unwrap();
        writeln!(output, "our total gas: {}", self.local_total_gas).unwrap();
        writeln!(output, "score: {score}").unwrap();
        writeln!(output, "\ndelta (best known - ours):").unwrap();
        for (index, percentile) in PERCENTILES.into_iter().enumerate() {
            if index != 0 {
                output.push_str("  ");
            }
            write!(output, "p{percentile}: {:+}", nearest_rank(&self.deltas, percentile)).unwrap();
        }
        writeln!(output).unwrap();
        writeln!(output, "\nimproved: {} graphs, {} gas saved", self.improvements, self.gas_saved)
            .unwrap();
        writeln!(output, "search capped: {} graphs", self.capped_searches).unwrap();
        write!(output, "elapsed: {:.2}s", elapsed.as_secs_f64()).unwrap();
        output
    }
}

fn nearest_rank(sorted: &[i64], percentile: usize) -> i64 {
    assert!(!sorted.is_empty());
    assert!((1..=100).contains(&percentile));
    let rank =
        percentile.checked_mul(sorted.len()).expect("percentile rank overflow").div_ceil(100);
    sorted[rank - 1]
}

#[cfg(test)]
mod tests {
    use super::*;
    use plank_test_utils::dedent_preserve_indent;

    #[test]
    fn renders_the_complete_summary_with_nearest_rank_percentiles() {
        let mut stats = Stats::new(5);
        stats.record(10, 15, false);
        stats.record(10, 12, true);
        stats.record(10, 10, false);
        stats.record(10, 8, false);
        stats.record(10, 3, false);

        let expected = dedent_preserve_indent(
            r#"
            graphs: 5
            best known total gas: 50
            our total gas: 48
            score: 104.17%

            delta (best known - ours):
            p5: -5  p20: -5  p50: +0  p80: +2  p95: +7

            improved: 2 graphs, 9 gas saved
            search capped: 1 graphs
            elapsed: 1.25s
            "#,
        );
        assert_eq!(stats.render(Duration::from_millis(1250)), expected);
    }
}

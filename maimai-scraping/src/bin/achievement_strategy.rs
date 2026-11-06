use std::{fmt::Display, path::PathBuf};

use anyhow::Result;
use clap::Parser;
use itertools::Itertools;
use maimai_scraping::maimai::rating::ScoreLevel;
use maimai_scraping_utils::{fs_json_util::read_json, regex};

use crate::score_solver::{solve, Costs};

#[derive(Parser)]
struct Opts {
    json_path: PathBuf,
    target_score: i64,
    #[arg(long, default_value = "100")]
    show_count: usize,
    #[arg(long, default_value = "4")]
    digits: u32,
    #[arg(long, default_value = "500")]
    level_coefficient: u64,
    #[arg(long, default_value = "0")]
    min_level: u64,
    #[arg(long, default_value = "150")]
    max_level: u64,
}

fn main() -> Result<()> {
    let opts = Opts::parse();
    let songs: Vec<serde_json::Map<String, serde_json::Value>> = read_json(&opts.json_path)?;

    let mut scores = vec![];
    for song in &songs {
        let title = song.get("title").unwrap().as_str().unwrap();
        let artist = song.get("artist").unwrap().as_str().unwrap();

        for dx in [false, true] {
            for diff in ["bas", "adv", "exp", "mas", "remas"] {
                let dx_prefix = if dx { "dx_" } else { "" };
                let count = ["tap", "hold", "slide", "touch", "break"].map(|kind| {
                    song.get(&format!("{dx_prefix}lev_{diff}_notes_{kind}"))
                        .and_then(|x| x.as_str())
                        .and_then(|x| x.parse().ok())
                });
                if count.iter().all(|x| x.is_none()) {
                    continue;
                }
                if (0..5).all(|i| count[i].is_some() == !(!dx || i == 3)) {
                    println!("Invalid data: {title}");
                    continue;
                }
                let Some(level) = song
                    .get(&format!("{dx_prefix}lev_{diff}_i"))
                    .and_then(|x| x.as_str())
                    .and_then(|x| regex!(r"([1-9]|1[0-5]).([0-9])").captures(x))
                    .map(|level| {
                        level[1].parse::<u64>().unwrap() * 10 + level[2].parse::<u64>().unwrap()
                    })
                    .or_else(|| {
                        Some(u8::from(
                            song.get(&format!("{dx_prefix}lev_{diff}"))?
                                .as_str()?
                                .parse::<ScoreLevel>()
                                .ok()?
                                .score_constant_candidates()
                                .next()?,
                        ) as u64)
                    })
                else {
                    println!("Invalid level: {title} {dx} {diff}");
                    continue;
                };

                if (opts.min_level..=opts.max_level).contains(&level) {
                    scores.push(Score {
                        title,
                        artist,
                        level,
                        dx,
                        diff,
                        notes: count.map(|x| x.unwrap_or_default()),
                    });
                }
            }
        }
    }

    let c = Costs::default();

    let mut results = vec![];
    for score in &scores {
        results.push((score, solve(c, opts.target_score, score.notes).unwrap()));
    }
    let mut candidates = vec![];
    for (score, result) in &results {
        for candidate in &result.candidates {
            candidates.push((score, result, candidate));
        }
    }
    candidates.sort_by_key(|x| x.2.cost + opts.level_coefficient * x.0.level);

    for (score, _result, solution) in candidates {
        println!(
            "Song : {score} (Lv. {}.{})",
            score.level / 10,
            score.level % 10
        );
        println!("Cost : {}", solution.cost);
        println!("    PERFECT GREAT GOOD MISS");
        for (kind, count) in ["Tap", "Hold", "Slide", "Touch"]
            .iter()
            .zip_eq(solution.first_four)
        {
            println!("{kind:5}: {count:3?}");
        }
        println!("   CRITICAL ┌PERFECT┐  ┌┈┈ GREAT ┈┈┐ GOOD MISS");
        println!("Break: {:3?}", solution.fifth);

        type U = u128;
        let mut note_score = 0;
        let mut note_sum = 0;
        let mut bonus_score = 0;
        let mut bonus_sum = 0;
        for i in 0..4 {
            let weight = [1, 2, 3, 1][i];
            let [p, g, o, m] = solution.first_four[i].map(|x| x as U);
            note_score += weight * (10 * p + 8 * g + 5 * o);
            note_sum += weight * 10 * (p + g + o + m);
        }
        {
            let [cp, p1, p2, g1, g2, g3, o, m] = solution.fifth.map(|x| x as U);
            let count = cp + p1 + p2 + g1 + g2 + g3 + o + m;
            note_score += 50 * (cp + p1 + p2) + 40 * g1 + 30 * g2 + 25 * g3 + 20 * o;
            note_sum += 50 * count;
            bonus_score += 20 * cp + 15 * p1 + 10 * p2 + 8 * (g1 + g2 + g3) + 6 * o;
            bonus_sum += 20 * count;
        }
        // (note_score / note_sum * 100 + bonus_score / bonus_sum)
        let num = note_score * 100 * bonus_sum + bonus_score * note_sum;
        let den = note_sum * bonus_sum;
        let conv = |d| {
            let ten = (10 as U).pow(d);
            let res = ten.checked_mul(num).unwrap() / den;
            (res, res / ten, res % ten)
        };
        let (_, x, y) = conv(opts.digits);
        println!(
            "Score: {x}.{y:0$}% ({note_score}/{note_sum} + {bonus_score}/{bonus_sum})",
            opts.digits as usize
        );
        assert_eq!(conv(4).0 as i64, opts.target_score);

        println!();
    }
    // for solution in result.candidates {
    //     println!("Cost: {}", solution.cost);
    //     println!("First four rows: {:?}", solution.first_four);
    //     println!("Fifth row: {:?}", solution.fifth);
    // }

    // println!("Feasibility: {:?}", result.feasibility);
    // println!("Optimality proven: {}", result.optimality_proven);

    Ok(())
}

struct Score<'a> {
    title: &'a str,
    artist: &'a str,
    level: u64,
    dx: bool,
    diff: &'static str,
    notes: [u16; 5],
}
impl Display for Score<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.title)?;
        if self.title == "Link" {
            write!(f, " ({})", self.artist)?;
        }
        write!(f, " ({} {})", if self.dx { "dx" } else { "std" }, self.diff)?;
        write!(f, " {:?}", self.notes)?;
        Ok(())
    }
}

mod score_solver {
    //! Specialized exact-score / near-minimum-cost solver.
    //!
    //! No external crates required (Rust 1.73+). Run the included tests with:
    //!   rustc --edition=2021 --test score_solver.rs -O -o score_solver_tests
    //!   ./score_solver_tests
    //!
    //! If a[4] == 0, the second score term is defined to be zero.
    //! If every a[i] == 0, the score is defined to be zero.
    //! Search limits may yield Feasibility::Unknown; every returned candidate
    //! nonetheless satisfies the original score exactly.

    use std::cmp::Reverse;
    use std::collections::BinaryHeap;

    const ROW_WEIGHT: [i64; 4] = [1, 2, 3, 1];
    const T: [i64; 4] = [10, 8, 5, 0];
    const U: [i64; 8] = [50, 50, 50, 40, 30, 25, 20, 0];
    const V: [i64; 8] = [20, 15, 10, 8, 8, 8, 6, 0];

    #[derive(Clone, Copy, Debug)]
    pub struct Costs {
        pub first_four: [[u32; 4]; 4],
        pub fifth: [u32; 8],
    }

    impl Default for Costs {
        fn default() -> Self {
            Self {
                first_four: [[0, 20, 20, 0], [0, 20, 5, 0], [0, 20, 5, 0], [0, 20, 20, 0]],
                fifth: [10, 50, 50, 100, 100, 100, 50, 0],
            }
        }
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum Feasibility {
        Feasible,
        Infeasible, // Mathematically proven impossible.
        Unknown,    // Search budget or middle-category limit prevented a proof.
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct Solution {
        pub first_four: [[u16; 4]; 4],
        pub fifth: [u16; 8],
        pub cost: u64,
    }

    #[derive(Clone, Debug, Default)]
    pub struct SolveStats {
        pub first_four_patterns: usize,
        pub fifth_states_expanded: usize,
        pub fifth_k_checked: usize,
        pub budget_exhausted: bool,
    }

    #[derive(Clone, Debug)]
    pub struct SolveResult {
        pub feasibility: Feasibility,
        pub candidates: Vec<Solution>, // In nondecreasing cost order.
        pub optimality_proven: bool,   // Applies to candidates[0], if present.
        pub stats: SolveStats,
    }

    impl Default for SolveResult {
        fn default() -> Self {
            Self {
                feasibility: Feasibility::Unknown,
                candidates: Vec::new(),
                optimality_proven: false,
                stats: SolveStats::default(),
            }
        }
    }

    #[derive(Clone, Copy, Debug)]
    pub struct SolveOptions {
        pub max_middle_units: u16,
        pub max_states: usize,
        pub max_k_checks: usize,
        pub max_candidates: usize,
        pub candidate_cost_slack: u64,
    }

    impl Default for SolveOptions {
        fn default() -> Self {
            Self {
                max_middle_units: 12,
                max_states: 800,
                max_k_checks: 20_000,
                max_candidates: 4,
                candidate_cost_slack: 80,
            }
        }
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum SolveError {
        SumTooLarge,
        UnsupportedCosts,
        ZeroCandidateCapacity,
    }

    // Assumptions used for the fast algorithm:
    // - Rows 0..4 have zero-cost categories 0 and 3.
    // - In row 4, category 7 is no more expensive than category 0,
    //   and categories 1..6 are no cheaper than category 0.
    fn validate_costs(c: Costs) -> bool {
        c.first_four.iter().all(|row| row[0] == 0 && row[3] == 0)
            && c.fifth[7] <= c.fifth[0]
            && c.fifth[1..7].iter().all(|&value| value >= c.fifth[0])
    }

    #[derive(Clone, Copy, Debug)]
    struct Pattern {
        p: [u16; 4], // Number of category 1 (zero-based) items in each row.
        q: [u16; 4], // Number of category 2 items.
        remaining: [u16; 4],
        offset: usize, // Deficit from p and q, without category 3.
        cost: u64,
    }

    fn choices(size: u16) -> Vec<(u16, u16)> {
        let mut out = Vec::with_capacity(10);
        for p in 0..=4.min(size) {
            for q in 0..=1.min(size - p) {
                out.push((p, q));
            }
        }
        out
    }

    fn make_pattern(c: Costs, a: [u16; 5], p: [u16; 4], q: [u16; 4]) -> Pattern {
        let mut remaining = [0_u16; 4];
        let mut offset = 0_usize;
        let mut cost = 0_u64;
        for i in 0..4 {
            debug_assert!(p[i] + q[i] <= a[i]);
            remaining[i] = a[i] - p[i] - q[i];
            offset += ROW_WEIGHT[i] as usize * (2 * p[i] as usize + 5 * q[i] as usize);
            cost +=
                c.first_four[i][1] as u64 * p[i] as u64 + c.first_four[i][2] as u64 * q[i] as u64;
        }
        Pattern {
            p,
            q,
            remaining,
            offset,
            cost,
        }
    }

    fn build_patterns(c: Costs, a: [u16; 5]) -> Vec<Pattern> {
        let mut out = Vec::new();
        if c.first_four[0] == c.first_four[3] {
            // Merge the two s=1 rows. Any aggregate (p,q) can be split back.
            let group_a = choices(a[0] + a[3]);
            let group_b = choices(a[1]);
            let group_c = choices(a[2]);
            for &(pa, qa) in &group_a {
                let p0 = pa.min(a[0]);
                let p3 = pa - p0;
                let q0 = qa.min(a[0] - p0);
                let q3 = qa - q0;
                debug_assert!(p3 + q3 <= a[3]);
                for &(pb, qb) in &group_b {
                    for &(pc, qc) in &group_c {
                        out.push(make_pattern(c, a, [p0, pb, pc, p3], [q0, qb, qc, q3]));
                    }
                }
            }
        } else {
            // Still exact for different costs in rows 0 and 3 (<= 10^4 patterns).
            let r0 = choices(a[0]);
            let r1 = choices(a[1]);
            let r2 = choices(a[2]);
            let r3 = choices(a[3]);
            for &(p0, q0) in &r0 {
                for &(p1, q1) in &r1 {
                    for &(p2, q2) in &r2 {
                        for &(p3, q3) in &r3 {
                            out.push(make_pattern(c, a, [p0, p1, p2, p3], [q0, q1, q2, q3]));
                        }
                    }
                }
            }
        }
        out.sort_unstable_by_key(|p| p.cost);
        out
    }

    #[derive(Clone, Copy)]
    struct Progression {
        first: usize,
        last: usize,
        step: usize,
    }

    // Exactly represents {z_a + 2*z_b + 3*z_c: 0<=z_a<=a, ...}
    // as at most two arithmetic progressions. All steps are 1, 2, or 3.
    fn z_progressions(a: usize, b: usize, c: usize) -> Vec<Progression> {
        if a >= 1 && a + 2 * b >= 2 {
            vec![Progression {
                first: 0,
                last: a + 2 * b + 3 * c,
                step: 1,
            }]
        } else if a == 1 {
            vec![
                Progression {
                    first: 0,
                    last: 3 * c,
                    step: 3,
                },
                Progression {
                    first: 1,
                    last: 3 * c + 1,
                    step: 3,
                },
            ]
        } else if b <= 1 {
            (0..=b)
                .map(|k| Progression {
                    first: 2 * k,
                    last: 2 * k + 3 * c,
                    step: 3,
                })
                .collect()
        } else {
            let mut out = vec![Progression {
                first: 0,
                last: 2 * b + 6 * (c / 2),
                step: 2,
            }];
            if c >= 1 {
                out.push(Progression {
                    first: 3,
                    last: 3 + 2 * b + 6 * ((c - 1) / 2),
                    step: 2,
                });
            }
            out
        }
    }

    // DSU over residue classes mod 60. Once an index is assigned, point to
    // the next unassigned index with the same remainder mod 60.
    fn next_unassigned(parent: &mut [usize], index: usize) -> usize {
        let mut root = index;
        while parent[root] != root {
            root = parent[root];
        }
        let mut node = index;
        while parent[node] != node {
            let next = parent[node];
            parent[node] = root;
            node = next;
        }
        root
    }

    fn make_first_four_lookup(patterns: &[Pattern], max_deficit: usize) -> Vec<u16> {
        const NONE: u16 = u16::MAX;
        let mut best = vec![NONE; max_deficit + 1];
        let mut parent: Vec<usize> = (0..max_deficit + 61).collect();

        for (pattern_id, pattern) in patterns.iter().enumerate() {
            debug_assert!(pattern_id < NONE as usize);
            let a = (pattern.remaining[0] + pattern.remaining[3]) as usize;
            let b = pattern.remaining[1] as usize;
            let c = pattern.remaining[2] as usize;
            for progression in z_progressions(a, b, c) {
                let first = pattern.offset + 10 * progression.first;
                let last = pattern.offset + 10 * progression.last;
                let step = 10 * progression.step; // 10, 20, or 30; divides 60.
                debug_assert!(last <= max_deficit && 60 % step == 0);
                for residue in 0..(60 / step) {
                    let start = first + residue * step;
                    if start > last {
                        continue;
                    }
                    let mut current = next_unassigned(&mut parent, start);
                    while current <= last {
                        best[current] = pattern_id as u16;
                        parent[current] = next_unassigned(&mut parent, current + 60);
                        current = next_unassigned(&mut parent, current);
                    }
                }
            }
        }
        best
    }

    fn reconstruct_first_four(pattern: Pattern, deficit: usize) -> [[u16; 4]; 4] {
        let t = (deficit - pattern.offset) / 10;
        let a = (pattern.remaining[0] + pattern.remaining[3]) as usize;
        let b = pattern.remaining[1] as usize;
        let c = pattern.remaining[2] as usize;

        let c_min = t.saturating_sub(a + 2 * b).div_ceil(3);
        let c_max = c.min(t / 3);
        for zc in (c_min..=c_max).rev() {
            let r = t - 3 * zc;
            let b_min = r.saturating_sub(a).div_ceil(2);
            let b_max = b.min(r / 2);
            if b_min <= b_max {
                let zb = b_min;
                let za = r - 2 * zb;
                let z0 = za.min(pattern.remaining[0] as usize);
                let z3 = za - z0;
                let z = [z0 as u16, zb as u16, zc as u16, z3 as u16];
                let mut x = [[0_u16; 4]; 4];
                for i in 0..4 {
                    x[i] = [
                        pattern.remaining[i] - z[i],
                        pattern.p[i],
                        pattern.q[i],
                        z[i],
                    ];
                }
                return x;
            }
        }
        unreachable!("first-four-row lookup stored an unreachable deficit");
    }

    fn gcd(mut a: i64, mut b: i64) -> i64 {
        while b != 0 {
            (a, b) = (b, a % b);
        }
        a.abs()
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
    struct MiddleState {
        y: [u16; 6], // Fifth-row categories 1..6.
        count: u16,
        start: u8, // Only append categories >= start; avoids duplicate vectors.
        weight: i64,
        extra_cost: i64,
    }

    fn lower_bound(
        state: MiddleState,
        a5: u16,
        rb: i64,
        last_weight: i64,
        c: Costs,
    ) -> Option<(u16, u64)> {
        if state.count > a5 || state.weight > rb {
            return None;
        }
        let available = a5 - state.count;
        let k = ((rb - state.weight) / last_weight).min(available as i64) as u16;
        let bound = c.fifth[0] as i64 * a5 as i64
            + (c.fifth[7] as i64 - c.fifth[0] as i64) * k as i64
            + state.extra_cost;
        debug_assert!(bound >= 0);
        Some((k, bound as u64))
    }

    fn insert_candidate(out: &mut Vec<Solution>, candidate: Solution, capacity: usize) {
        if out.len() == capacity && candidate.cost >= out.last().unwrap().cost {
            return;
        }
        let pos = out.partition_point(|solution| solution.cost <= candidate.cost);
        out.insert(pos, candidate);
        out.truncate(capacity);
    }

    pub fn solve(c: Costs, target_score: i64, a: [u16; 5]) -> Result<SolveResult, SolveError> {
        solve_with_options(c, target_score, a, SolveOptions::default())
    }

    pub fn solve_with_options(
        c: Costs,
        target_score: i64,
        a: [u16; 5],
        options: SolveOptions,
    ) -> Result<SolveResult, SolveError> {
        if a.iter().map(|&x| x as usize).sum::<usize>() > 2500 {
            return Err(SolveError::SumTooLarge);
        }
        if !validate_costs(c) {
            return Err(SolveError::UnsupportedCosts);
        }
        if options.max_candidates == 0 {
            return Err(SolveError::ZeroCandidateCapacity);
        }

        let mut result = SolveResult::default();
        let a5 = a[4];
        let maximum_score = if a5 == 0 { 1_000_000 } else { 1_010_000 };
        if !(0..=maximum_score).contains(&target_score) {
            result.feasibility = Feasibility::Infeasible;
            return Ok(result);
        }

        let d = (0..4)
            .map(|i| 10 * ROW_WEIGHT[i] * a[i] as i64)
            .sum::<i64>()
            + 50 * a5 as i64;
        if d == 0 {
            // Convention: for all-zero a, the score is defined to be zero.
            result.feasibility = if target_score == 0 {
                Feasibility::Feasible
            } else {
                Feasibility::Infeasible
            };
            if target_score == 0 {
                result.optimality_proven = true;
                result.candidates.push(Solution {
                    first_four: [[0; 4]; 4],
                    fifth: [0; 8],
                    cost: 0,
                });
            }
            return Ok(result);
        }

        let max_deficit = (0..4)
            .map(|i| 10 * ROW_WEIGHT[i] as usize * a[i] as usize)
            .sum::<usize>();
        let scale = 1_000_000_i64 * a5 as i64;
        let b = d * a5 as i64;
        let weights: [i64; 8] =
            std::array::from_fn(|j| scale * (50 - U[j]) + 500 * d * (20 - V[j]));

        if a5 > 0 {
            // Necessary lattice condition for integer deficits.
            let r = 1_010_000 - target_score;
            let rb = r * b;
            let mut divisor = 0_i64;
            for i in 0..4 {
                if a[i] > 0 {
                    divisor = gcd(divisor, 2 * scale * ROW_WEIGHT[i]);
                    divisor = gcd(divisor, 5 * scale * ROW_WEIGHT[i]);
                }
            }
            for &weight in &weights[1..] {
                divisor = gcd(divisor, weight);
            }
            if (rb / divisor) * divisor <= (r - 1) * b {
                result.feasibility = Feasibility::Infeasible;
                return Ok(result);
            }
        }

        let patterns = build_patterns(c, a);
        result.stats.first_four_patterns = patterns.len();
        let best = make_first_four_lookup(&patterns, max_deficit);
        const NONE: u16 = u16::MAX;

        if a5 == 0 {
            // Second term is defined as zero. At most one L4 can fit.
            let r = 1_000_000 - target_score;
            let u = r * d;
            let deficit = (u / 1_000_000) as usize;
            if u % 1_000_000 < d && deficit <= max_deficit && best[deficit] != NONE {
                let pattern = patterns[best[deficit] as usize];
                result.candidates.push(Solution {
                    first_four: reconstruct_first_four(pattern, deficit),
                    fifth: [0; 8],
                    cost: pattern.cost,
                });
                result.feasibility = Feasibility::Feasible;
                result.optimality_proven = true;
            } else {
                result.feasibility = Feasibility::Infeasible;
            }
            return Ok(result);
        }

        let r = 1_010_000 - target_score;
        let rb = r * b;
        let last_weight = weights[7];
        let cap = options.max_middle_units.min(a5);
        let first = MiddleState {
            y: [0; 6],
            count: 0,
            start: 0,
            weight: 0,
            extra_cost: 0,
        };
        let (_, root_lb) = lower_bound(first, a5, rb, last_weight, c).unwrap();
        let min_extra_cost = (1..7).map(|j| c.fifth[j] - c.fifth[0]).min().unwrap() as u64;
        let outside_cap_lb = if cap < a5 {
            root_lb + min_extra_cost * (cap as u64 + 1)
        } else {
            u64::MAX
        };

        // Min-heap ordered by the lower bound on the fifth-row cost.
        let mut heap: BinaryHeap<Reverse<(u64, u64, MiddleState)>> = BinaryHeap::new();
        let mut serial = 0_u64;
        heap.push(Reverse((root_lb, serial, first)));
        let mut frontier_lb = u64::MAX;
        let mut budget_exhausted = false;

        'search: while let Some(Reverse((bound, _, state))) = heap.pop() {
            if let Some(best_cost) = result.candidates.first().map(|x| x.cost) {
                if bound > best_cost.saturating_add(options.candidate_cost_slack) {
                    frontier_lb = bound;
                    break;
                }
            }
            if result.stats.fifth_states_expanded == options.max_states {
                frontier_lb = bound;
                budget_exhausted = true;
                break;
            }
            result.stats.fifth_states_expanded += 1;
            let (k_max, _) = lower_bound(state, a5, rb, last_weight, c).unwrap();

            // With L4 <= max_deficit, values of k below this threshold
            // cannot make the score large enough.
            let needed = (r - 1) * b - state.weight - scale * max_deficit as i64;
            let k_min = if needed < 0 {
                0
            } else {
                needed / last_weight + 1
            };
            if k_min <= k_max as i64 {
                for k in ((k_min as u16)..=k_max).rev() {
                    if result.stats.fifth_k_checked == options.max_k_checks {
                        frontier_lb = bound;
                        budget_exhausted = true;
                        break 'search;
                    }
                    result.stats.fifth_k_checked += 1;
                    let cost5 = (c.fifth[0] as i64 * a5 as i64
                        + (c.fifth[7] as i64 - c.fifth[0] as i64) * k as i64
                        + state.extra_cost) as u64;
                    if let Some(best_cost) = result.candidates.first().map(|x| x.cost) {
                        if cost5 > best_cost.saturating_add(options.candidate_cost_slack) {
                            break;
                        }
                    }
                    let u = rb - state.weight - last_weight * k as i64;
                    debug_assert!(u >= 0);
                    if u % scale >= b {
                        continue;
                    }
                    let deficit = (u / scale) as usize;
                    if deficit > max_deficit || best[deficit] == NONE {
                        continue;
                    }
                    let pattern = patterns[best[deficit] as usize];
                    let total_cost = cost5 + pattern.cost;
                    if result.candidates.len() == options.max_candidates
                        && total_cost >= result.candidates.last().unwrap().cost
                    {
                        continue;
                    }
                    let mut fifth = [0_u16; 8];
                    fifth[0] = a5 - state.count - k;
                    fifth[1..7].copy_from_slice(&state.y);
                    fifth[7] = k;
                    let candidate = Solution {
                        first_four: reconstruct_first_four(pattern, deficit),
                        fifth,
                        cost: total_cost,
                    };
                    debug_assert_eq!(exact_score(a, &candidate), target_score);
                    insert_candidate(&mut result.candidates, candidate, options.max_candidates);
                }
            }

            if state.count < cap {
                for j in state.start as usize..6 {
                    let mut child = state;
                    child.y[j] += 1;
                    child.count += 1;
                    child.start = j as u8;
                    child.weight += weights[j + 1];
                    child.extra_cost += (c.fifth[j + 1] - c.fifth[0]) as i64;
                    if let Some((_, child_lb)) = lower_bound(child, a5, rb, last_weight, c) {
                        if let Some(best_cost) = result.candidates.first().map(|x| x.cost) {
                            if child_lb > best_cost.saturating_add(options.candidate_cost_slack) {
                                continue;
                            }
                        }
                        serial += 1;
                        heap.push(Reverse((child_lb, serial, child)));
                    }
                }
            }
        }
        result.stats.budget_exhausted = budget_exhausted;
        // If stopped in the middle of a node, its lower bound is also
        // a lower bound on that unfinished part of the search.
        if let Some(Reverse((bound, _, _))) = heap.peek() {
            frontier_lb = frontier_lb.min(*bound);
        }
        if let Some(best_cost) = result.candidates.first().map(|x| x.cost) {
            result.feasibility = Feasibility::Feasible;
            result.optimality_proven = frontier_lb >= best_cost && outside_cap_lb >= best_cost;
        } else {
            result.feasibility = if !budget_exhausted && frontier_lb == u64::MAX && cap == a5 {
                Feasibility::Infeasible
            } else {
                Feasibility::Unknown
            };
        }
        Ok(result)
    }

    // Exact reference implementation of the original score, without floating point.
    // Also used for debug assertions; unlike the deficit check, this uses the
    // original numerator and v coefficients directly.
    pub fn exact_score(a: [u16; 5], solution: &Solution) -> i64 {
        let d = (0..4)
            .map(|i| 10 * ROW_WEIGHT[i] * a[i] as i64)
            .sum::<i64>()
            + 50 * a[4] as i64;
        if d == 0 {
            return 0;
        }
        let mut n = 0_i64;
        #[allow(clippy::needless_range_loop)]
        for i in 0..4 {
            #[allow(clippy::needless_range_loop)]
            for j in 0..4 {
                n += ROW_WEIGHT[i] * T[j] * solution.first_four[i][j] as i64;
            }
        }
        #[allow(clippy::needless_range_loop)]
        for j in 0..8 {
            n += U[j] * solution.fifth[j] as i64;
        }
        if a[4] == 0 {
            return 1_000_000 * n / d;
        }
        let v_total = (0..8).map(|j| V[j] * solution.fifth[j] as i64).sum::<i64>();
        let denominator = 20 * a[4] as i64 * d;
        let numerator = 1_000_000 * n * 20 * a[4] as i64 + 10_000 * v_total * d;
        numerator / denominator
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn zero_denominator_conventions() {
            let c = Costs::default();
            let all_zero = solve(c, 0, [0; 5]).unwrap();
            assert_eq!(all_zero.feasibility, Feasibility::Feasible);
            assert!(all_zero.optimality_proven);
            assert_eq!(all_zero.candidates[0].cost, 0);
            assert_eq!(
                solve(c, 1, [0; 5]).unwrap().feasibility,
                Feasibility::Infeasible
            );

            let no_fifth = solve(c, 1_000_000, [4, 2, 1, 0, 0]).unwrap();
            assert_eq!(no_fifth.feasibility, Feasibility::Feasible);
            assert!(no_fifth.optimality_proven);
            assert_eq!(
                exact_score([4, 2, 1, 0, 0], &no_fifth.candidates[0]),
                1_000_000
            );
        }

        #[test]
        fn progressions_match_brute_force() {
            for a in 0..10 {
                for b in 0..10 {
                    for c in 0..10 {
                        let mut actual = vec![false; a + 2 * b + 3 * c + 1];
                        for x in 0..=a {
                            for y in 0..=b {
                                for z in 0..=c {
                                    actual[x + 2 * y + 3 * z] = true;
                                }
                            }
                        }
                        let mut from_progressions = vec![false; actual.len()];
                        for p in z_progressions(a, b, c) {
                            for k in (p.first..=p.last).step_by(p.step) {
                                from_progressions[k] = true;
                            }
                        }
                        assert_eq!(actual, from_progressions, "a={a}, b={b}, c={c}");
                    }
                }
            }
        }

        #[test]
        fn lookup_matches_brute_force_dp() {
            let c = Costs::default();
            for a0 in 0..4 {
                for a1 in 0..4 {
                    for a2 in 0..4 {
                        for a3 in 0..4 {
                            let a = [a0, a1, a2, a3, 1];
                            let patterns = build_patterns(c, a);
                            let max_deficit = (0..4)
                                .map(|i| 10 * ROW_WEIGHT[i] as usize * a[i] as usize)
                                .sum::<usize>();
                            let lookup = make_first_four_lookup(&patterns, max_deficit);
                            let mut dp = vec![None::<u64>; max_deficit + 1];
                            dp[0] = Some(0);
                            for i in 0..4 {
                                let mut next = vec![None::<u64>; max_deficit + 1];
                                for x2 in 0..=a[i] {
                                    for x3 in 0..=a[i] - x2 {
                                        for x4 in 0..=a[i] - x2 - x3 {
                                            let deficit = ROW_WEIGHT[i] as usize
                                                * (2 * x2 as usize
                                                    + 5 * x3 as usize
                                                    + 10 * x4 as usize);
                                            let cost = c.first_four[i][1] as u64 * x2 as u64
                                                + c.first_four[i][2] as u64 * x3 as u64;
                                            for (old_deficit, old) in dp.iter().enumerate() {
                                                if let Some(old_cost) = old {
                                                    let slot = &mut next[old_deficit + deficit];
                                                    let new_cost = *old_cost + cost;
                                                    if slot
                                                        .is_none_or(|existing| new_cost < existing)
                                                    {
                                                        *slot = Some(new_cost);
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                                dp = next;
                            }
                            for (deficit, &expected) in dp.iter().enumerate() {
                                let got = if lookup[deficit] == u16::MAX {
                                    None
                                } else {
                                    let p = patterns[lookup[deficit] as usize];
                                    let x = reconstruct_first_four(p, deficit);
                                    for i in 0..4 {
                                        assert_eq!(x[i].iter().sum::<u16>(), a[i]);
                                    }
                                    Some(p.cost)
                                };
                                assert_eq!(got, expected, "a={a:?}, deficit={deficit}");
                            }
                        }
                    }
                }
            }
        }

        #[test]
        fn different_costs_in_equal_weight_rows() {
            // This exercises the four-row path instead of the merged three-group path.
            let mut c = Costs::default();
            c.first_four[3][1] = 17;
            let a = [3, 2, 1, 3, 1];
            let patterns = build_patterns(c, a);
            let limit = (0..4)
                .map(|i| 10 * ROW_WEIGHT[i] as usize * a[i] as usize)
                .sum::<usize>();
            let lookup = make_first_four_lookup(&patterns, limit);
            let mut dp = vec![None::<u64>; limit + 1];
            dp[0] = Some(0);
            for i in 0..4 {
                let mut next = vec![None::<u64>; limit + 1];
                for p in 0..=a[i] {
                    for q in 0..=a[i] - p {
                        for z in 0..=a[i] - p - q {
                            let deficit = ROW_WEIGHT[i] as usize
                                * (2 * p as usize + 5 * q as usize + 10 * z as usize);
                            let cost = c.first_four[i][1] as u64 * p as u64
                                + c.first_four[i][2] as u64 * q as u64;
                            for (l, old) in dp.iter().enumerate() {
                                if let Some(prev) = old {
                                    let entry = &mut next[l + deficit];
                                    let new_cost = *prev + cost;
                                    if entry.is_none_or(|old_cost| new_cost < old_cost) {
                                        *entry = Some(new_cost);
                                    }
                                }
                            }
                        }
                    }
                }
                dp = next;
            }
            for l in 0..=limit {
                let found = if lookup[l] == u16::MAX {
                    None
                } else {
                    Some(patterns[lookup[l] as usize].cost)
                };
                assert_eq!(found, dp[l], "deficit={l}");
            }
        }

        #[test]
        fn solver_matches_exhaustive_small_cases() {
            let c = Costs::default();
            for a5 in 0..=3_u16 {
                for a1 in 0..=3_u16 {
                    let a = [a1, 1, 1, 2, a5];
                    let patterns = build_patterns(c, a);
                    let max_deficit = (0..4)
                        .map(|i| 10 * ROW_WEIGHT[i] as usize * a[i] as usize)
                        .sum::<usize>();
                    let lookup = make_first_four_lookup(&patterns, max_deficit);
                    let mut expected = std::collections::BTreeMap::<i64, u64>::new();
                    // Enumerate all fifth rows and their exact best first-four row.
                    #[allow(clippy::too_many_arguments)]
                    fn enumerate_fifth(
                        j: usize,
                        left: u16,
                        fifth: &mut [u16; 8],
                        a: [u16; 5],
                        c: Costs,
                        patterns: &[Pattern],
                        lookup: &[u16],
                        expected: &mut std::collections::BTreeMap<i64, u64>,
                    ) {
                        if j == 7 {
                            fifth[7] = left;
                            for (l4, &idx) in lookup.iter().enumerate() {
                                if idx == u16::MAX {
                                    continue;
                                }
                                let p = patterns[idx as usize];
                                let candidate = Solution {
                                    first_four: reconstruct_first_four(p, l4),
                                    fifth: *fifth,
                                    cost: p.cost
                                        + (0..8)
                                            .map(|k| c.fifth[k] as u64 * fifth[k] as u64)
                                            .sum::<u64>(),
                                };
                                let score = exact_score(a, &candidate);
                                let entry = expected.entry(score).or_insert(u64::MAX);
                                *entry = (*entry).min(candidate.cost);
                            }
                        } else {
                            for count in 0..=left {
                                fifth[j] = count;
                                enumerate_fifth(
                                    j + 1,
                                    left - count,
                                    fifth,
                                    a,
                                    c,
                                    patterns,
                                    lookup,
                                    expected,
                                );
                            }
                        }
                    }
                    enumerate_fifth(0, a5, &mut [0; 8], a, c, &patterns, &lookup, &mut expected);
                    // Include absent targets between 0 and the maximum.
                    let stride = (expected.len() / 60).max(1);
                    for target in expected
                        .keys()
                        .copied()
                        .step_by(stride)
                        .chain([1, 12_345, 969_999, 1_010_000])
                    {
                        let result = solve_with_options(
                            c,
                            target,
                            a,
                            SolveOptions {
                                max_middle_units: a5,
                                max_states: usize::MAX,
                                max_k_checks: usize::MAX,
                                max_candidates: 3,
                                candidate_cost_slack: 0,
                            },
                        )
                        .unwrap();
                        match expected.get(&target) {
                            Some(&optimal_cost) => {
                                assert_eq!(
                                    result.feasibility,
                                    Feasibility::Feasible,
                                    "a={a:?}, s={target}"
                                );
                                assert!(result.optimality_proven, "a={a:?}, s={target}");
                                assert_eq!(
                                    result.candidates[0].cost, optimal_cost,
                                    "a={a:?}, s={target}"
                                );
                                assert_eq!(exact_score(a, &result.candidates[0]), target);
                            }
                            None => assert_eq!(
                                result.feasibility,
                                Feasibility::Infeasible,
                                "a={a:?}, s={target}"
                            ),
                        }
                    }
                }
            }
        }
    }
}

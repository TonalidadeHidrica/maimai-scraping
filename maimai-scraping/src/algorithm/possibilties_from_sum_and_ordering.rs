use std::cmp::Ordering;

use itertools::Itertools;

/// Consider a sequence `b[i]` of length `n` such that:
///
/// - `b[i]` is one of `a(i)`;
/// - `b[i]` is sorted in an ascending order w.r.t. `cmp`;
/// - Sum of `b[i].0` is `sum`.
///
/// Can `a(i).nth(j)` be `b[i]`?
///
/// # Constraints
/// - `a` and `c` must be pure.
///
/// # Returns
/// A vector `ret` of length `n`.
/// `ret[i]` is a subset of `a(i)` that can be `b[i]`.
///
/// ## Complexity
/// `O(n^2 m^2 Δa)`, where
/// - `m = max(a(i).count())`
/// - `Δa = max(a(i).last().unwrap().0 - a(i).first().unwrap().0)`
///
/// (because (`sum` in `solve_partial`) = `O(n Δa)`)
pub fn solve<T, F, I, C>(n: usize, a: F, cmp: C, sum: usize) -> Vec<Vec<(usize, T)>>
where
    F: Fn(usize) -> I,
    I: Iterator<Item = (usize, T)>,
    C: Fn(&(usize, T), &(usize, T)) -> Ordering,
{
    // println!(
    //     "solve({n}, {:?}, .., {sum})",
    //     (0..n).map(|i| a(i).collect_vec()).collect_vec(),
    // );

    if n == 0 {
        return vec![];
    }
    let Some(min_sum) = (0..n).map(|i| a(i).next()).fold_options(0, |s, x| s + x.0) else {
        return empty(n);
    };
    let max_sum = (0..n).map(|i| a(i).last().unwrap().0).sum();
    if !(min_sum..=max_sum).contains(&sum) {
        return empty(n);
    }
    let sum = sum - min_sum;
    let asc = solve_partial(n, &a, &cmp, sum);
    let dsc = solve_partial(n, |i| a(n - 1 - i), |x, y| cmp(x, y).reverse(), sum);
    let mut res = (0..n).map(|i| vec![false; a(i).count()]).collect_vec();
    for i in 0..n {
        let min_score = a(i).next().unwrap().0;
        for (j, (score, _)) in a(i).enumerate() {
            let score = score - min_score;
            if let Some(remain) = sum.checked_sub(score) {
                for s in 0..=remain {
                    // println!(
                    //     "i={i} j={j} score={score} remain={remain} s={s} asc={} dsc={}",
                    //     asc[i][s + score][j],
                    //     dsc[n - 1 - i][remain - s + score][j],
                    // );
                    res[i][j] |= asc[i][s + score][j] && dsc[n - 1 - i][remain - s + score][j];
                }
            }
        }
    }
    res.iter()
        .enumerate()
        .map(|(i, res)| {
            a(i).zip(res)
                .filter_map(|(elem, ok)| ok.then_some(elem))
                .collect()
        })
        .collect()
}

/// ## Constraints
/// - All constraints of `solve`.
/// - `n > 0`.
/// - `a(i)` is not empty.
///
/// ## Returns
/// `ret[i][s][j]`: Can `a(i).nth(j)` be the i-th element, the prefix until whom has the sum `s`?
///
/// ## Complexity
/// `O(n m^2 sum)`, where `m = max(a(i).count())`
fn solve_partial<T, F, I, C>(n: usize, a: F, cmp: C, sum: usize) -> Vec<Vec<Vec<bool>>>
where
    F: Fn(usize) -> I,
    I: Iterator<Item = (usize, T)>,
    C: Fn(&(usize, T), &(usize, T)) -> Ordering,
{
    let mut dp = empty(sum + 1);
    dp[0].push(None);
    let mut ret = vec![];
    for i in 0..n {
        let mut new = vec![vec![false; a(i).count()]; sum + 1];
        let min_score = a(i).next().unwrap().0;
        for (prev_sum, prevs) in dp.iter().enumerate() {
            for prev in prevs {
                for (j, (this_score, this)) in a(i).enumerate() {
                    let new_sum = prev_sum + (this_score - min_score);
                    if prev
                        .as_ref()
                        .is_none_or(|prev| cmp(prev, &(this_score, this)).is_lt())
                        && new_sum <= sum
                    {
                        new[new_sum][j] = true;
                    }
                }
            }
        }
        dp = new
            .iter()
            .map(|ok| {
                a(i).zip(ok)
                    .filter_map(|(elem, ok)| ok.then_some(Some(elem)))
                    .collect_vec()
            })
            .collect();
        ret.push(new);
    }
    // println!("{ret:?}");
    ret
}

fn empty<T>(n: usize) -> Vec<Vec<T>> {
    (0..n).map(|_| vec![]).collect()
}

#[cfg(test)]
mod tests {
    use super::solve;

    fn test_ordinary(a: Vec<Vec<usize>>, sum: usize, ans: Vec<Vec<usize>>) {
        let ans: Vec<Vec<_>> = ans
            .iter()
            .map(|ans| ans.iter().map(|&a| (a, ())).collect())
            .collect();
        let res = solve(
            a.len(),
            |i| a[i].iter().map(|&x| (x, ())),
            |x, y| x.cmp(y),
            sum,
        );
        assert_eq!(res, ans);
    }

    #[test]
    fn test_solve() {
        test_ordinary(vec![vec![0, 1], vec![2, 3]], 0, vec![vec![], vec![]]);
        test_ordinary(vec![vec![0, 1], vec![2, 3]], 1, vec![vec![], vec![]]);
        test_ordinary(vec![vec![0, 1], vec![2, 3]], 2, vec![vec![0], vec![2]]);
        test_ordinary(
            vec![vec![0, 1], vec![2, 3]],
            3,
            vec![vec![0, 1], vec![2, 3]],
        );
        test_ordinary(vec![vec![0, 1], vec![2, 3]], 4, vec![vec![1], vec![3]]);
        test_ordinary(vec![vec![0, 1], vec![2, 3]], 5, vec![vec![], vec![]]);

        test_ordinary(vec![vec![0, 3], vec![7, 11]], 11, vec![vec![0], vec![11]]);

        test_ordinary(vec![vec![1, 4], vec![3, 10]], 7, vec![vec![], vec![]]);
        test_ordinary(vec![vec![1, 4], vec![3, 10]], 11, vec![vec![1], vec![10]]);

        test_ordinary(
            vec![vec![100, 102, 104], vec![200, 203], vec![300, 304, 308]],
            604,
            vec![vec![100, 104], vec![200], vec![300, 304]],
        );

        // TODO: many of these Vectors can actually be arrays
        // Clippy pointed out some of them so I fixed them, but there should be more
        let a = [vec![210, 212], vec![208, 210]];
        let b = &[975132, 985146];
        let res = solve(
            2,
            |i| a[i].iter().map(move |&a| (a, b[i])),
            |x, y| x.0.cmp(&y.0).then_with(|| x.1.cmp(&y.1)).reverse(),
            420,
        );
        let expected = vec![vec![(212, 975132)], vec![(208, 985146)]];
        assert_eq!(res, expected);

        let a = [vec![210, 212], vec![208, 210]];
        let b = &[985146, 975132];
        let res = solve(
            2,
            |i| a[i].iter().map(move |&a| (a, b[i])),
            |x, y| x.0.cmp(&y.0).then_with(|| x.1.cmp(&y.1)).reverse(),
            420,
        );
        let expected = vec![
            vec![(210, 985146), (212, 985146)],
            vec![(208, 975132), (210, 975132)],
        ];
        assert_eq!(res, expected);
    }
}

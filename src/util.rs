//! Numeric helpers that reproduce Python's behavior exactly, so the Rust
//! build matches the Python build byte for byte.

/// Python's `round(x, places)`: the exact binary value rounded to the nearest
/// decimal, ties to even. Going through the decimal formatter gets the same
/// result, where `(x * 100).round() / 100` would not.
pub fn round_to(x: f64, places: usize) -> f64 {
    format!("{x:.places$}").parse().unwrap_or(x)
}

pub fn round2(x: f64) -> f64 {
    round_to(x, 2)
}

/// Python's `statistics.median` for a list of floats (average of the two
/// middle values when the count is even).
pub fn median(values: &[f64]) -> f64 {
    let mut s = values.to_vec();
    s.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = s.len();
    if n == 0 {
        return 0.0;
    }
    let mid = n / 2;
    if n % 2 == 1 {
        s[mid]
    } else {
        (s[mid - 1] + s[mid]) / 2.0
    }
}

/// Python's `max()` over floats: the first of the largest values. Only a
/// strictly larger value replaces it, so a tie between `0.0` and `-0.0` keeps
/// whichever came first, where `f64::max` may pick either.
pub fn py_max(values: impl IntoIterator<Item = f64>) -> Option<f64> {
    let mut values = values.into_iter();
    let first = values.next()?;
    Some(values.fold(first, |best, v| if v > best { v } else { best }))
}

/// Python's `min()` over floats: the first of the smallest values.
pub fn py_min(values: impl IntoIterator<Item = f64>) -> Option<f64> {
    let mut values = values.into_iter();
    let first = values.next()?;
    Some(values.fold(first, |best, v| if v < best { v } else { best }))
}

/// `math.fsum`: the correctly rounded sum, ported from CPython's
/// `math_fsum` (Shewchuk's partials, then a half-even correction across the
/// top partials so the result is the nearest double to the exact sum).
pub fn fsum(values: impl IntoIterator<Item = f64>) -> f64 {
    let mut partials: Vec<f64> = Vec::new();
    for mut x in values {
        let mut i = 0;
        for j in 0..partials.len() {
            let mut y = partials[j];
            if x.abs() < y.abs() {
                std::mem::swap(&mut x, &mut y);
            }
            let hi = x + y;
            let lo = y - (hi - x);
            if lo != 0.0 {
                partials[i] = lo;
                i += 1;
            }
            x = hi;
        }
        partials.truncate(i);
        if x != 0.0 {
            partials.push(x);
        }
    }

    let mut n = partials.len();
    if n == 0 {
        return 0.0;
    }
    n -= 1;
    let mut hi = partials[n];
    let mut lo = 0.0;
    // Add the partials from the top until the sum stops being exact.
    while n > 0 {
        let x = hi;
        n -= 1;
        let y = partials[n];
        hi = x + y;
        lo = y - (hi - x);
        if lo != 0.0 {
            break;
        }
    }
    // Round half-even across partials: a remainder of exactly half an ulp
    // pushed further by the next partial rounds away.
    if n > 0 && ((lo < 0.0 && partials[n - 1] < 0.0) || (lo > 0.0 && partials[n - 1] > 0.0)) {
        let y = lo * 2.0;
        let x = hi + y;
        if y == x - hi {
            hi = x;
        }
    }
    hi
}

/// Python's `x ** 0.5`. CPython calls libm `pow`, which can differ from
/// `sqrt` in the last bit; `black_box` keeps the compiler from turning this
/// into a `sqrt`.
pub fn pow_half(x: f64) -> f64 {
    x.powf(std::hint::black_box(0.5))
}

/// Python's `max(items, key=...)`: the first item whose key is largest.
/// (`Iterator::max_by` returns the last one on a tie.)
pub fn first_max_by<T>(
    items: impl IntoIterator<Item = T>,
    cmp: impl Fn(&T, &T) -> std::cmp::Ordering,
) -> Option<T> {
    items.into_iter().fold(None, |best, item| match best {
        Some(b) if cmp(&item, &b) != std::cmp::Ordering::Greater => Some(b),
        _ => Some(item),
    })
}

/// `statistics.fmean`.
pub fn fmean(values: &[f64]) -> f64 {
    fsum(values.iter().copied()) / values.len() as f64
}

/// `statistics.pvariance` (population variance).
pub fn pvariance(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mean = fmean(values);
    fsum(values.iter().map(|v| (v - mean) * (v - mean))) / values.len() as f64
}

/// Python's `int(float)`: truncation toward zero.
pub fn trunc(x: f64) -> i64 {
    x as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_matches_python_ties_and_binary_error() {
        assert_eq!(round2(2.675), 2.67); // 2.675 is stored just below the tie
        assert_eq!(round2(0.125), 0.12); // exact tie goes to the even digit
        assert_eq!(round2(1.005), 1.0);
        assert_eq!(round2(-0.001), -0.0);
    }

    #[test]
    fn median_averages_even_counts() {
        assert_eq!(median(&[3.0, 1.0, 2.0]), 2.0);
        assert_eq!(median(&[4.0, 1.0, 2.0, 3.0]), 2.5);
    }

    #[test]
    fn extremes_keep_the_first_of_equal_values_like_python() {
        assert!(py_max([0.0, -0.0]).unwrap().is_sign_positive());
        assert!(py_max([-0.0, 0.0]).unwrap().is_sign_negative());
        assert!(py_min([0.0, -0.0]).unwrap().is_sign_positive());
        assert_eq!(py_max([1.0, 3.0, 2.0]), Some(3.0));
        assert_eq!(py_min(std::iter::empty()), None);
    }

    #[test]
    fn fsum_is_exact_where_naive_sum_is_not() {
        assert_eq!(fsum([1e16, 1.0, -1e16]), 1.0);
    }

    #[test]
    fn fsum_matches_cpython_math_fsum() {
        // Values printed by CPython 3.12's math.fsum.
        assert_eq!(fsum([59.07, 155.64, 103.89]), 318.59999999999997);
        assert_eq!(fsum([1e-16, 1.0, 1e16]), 1.0000000000000002e16);
        assert_eq!(fsum([0.1; 10]), 1.0);
        assert_eq!(fsum(std::iter::empty()), 0.0);
    }

    #[test]
    fn pow_half_matches_python_rather_than_sqrt() {
        assert_eq!(pow_half(617.298836), 24.84549931074037); // sqrt gives ...365
    }

    #[test]
    fn first_max_by_keeps_the_first_of_equal_maxima() {
        let items = [(1, "a"), (3, "b"), (3, "c"), (2, "d")];
        let best = first_max_by(items.iter(), |a, b| a.0.cmp(&b.0)).unwrap();
        assert_eq!(best.1, "b");
    }
}

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

/// Exactly rounded sum, in the spirit of `math.fsum`: partials are kept so no
/// precision is lost to intermediate rounding, then combined top-down.
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
        partials.push(x);
    }
    partials.iter().rev().fold(0.0, |acc, p| acc + p)
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
}

//! Python's `random.Random`: the Mersenne Twister and `gauss` exactly as
//! CPython implements them, so seeded simulations reproduce Python's results.

const N: usize = 624;
const M: usize = 397;

pub struct PyRandom {
    mt: [u32; N],
    index: usize,
    gauss_next: Option<f64>,
}

impl PyRandom {
    /// `random.Random(seed)` for a non-negative integer seed.
    pub fn new(seed: u64) -> Self {
        let mut key = Vec::new();
        let mut n = seed;
        loop {
            key.push((n & 0xffff_ffff) as u32);
            n >>= 32;
            if n == 0 {
                break;
            }
        }
        let mut rng = PyRandom {
            mt: [0; N],
            index: N,
            gauss_next: None,
        };
        rng.init_by_array(&key);
        rng
    }

    fn init_genrand(&mut self, s: u32) {
        self.mt[0] = s;
        for i in 1..N {
            let prev = self.mt[i - 1];
            self.mt[i] = 1_812_433_253u32
                .wrapping_mul(prev ^ (prev >> 30))
                .wrapping_add(i as u32);
        }
        self.index = N;
    }

    fn init_by_array(&mut self, key: &[u32]) {
        self.init_genrand(19_650_218);
        let (mut i, mut j) = (1usize, 0usize);
        let mut k = N.max(key.len());
        while k > 0 {
            let prev = self.mt[i - 1];
            self.mt[i] = (self.mt[i] ^ (prev ^ (prev >> 30)).wrapping_mul(1_664_525))
                .wrapping_add(key[j])
                .wrapping_add(j as u32);
            i += 1;
            j += 1;
            if i >= N {
                self.mt[0] = self.mt[N - 1];
                i = 1;
            }
            if j >= key.len() {
                j = 0;
            }
            k -= 1;
        }
        k = N - 1;
        while k > 0 {
            let prev = self.mt[i - 1];
            self.mt[i] = (self.mt[i] ^ (prev ^ (prev >> 30)).wrapping_mul(1_566_083_941))
                .wrapping_sub(i as u32);
            i += 1;
            if i >= N {
                self.mt[0] = self.mt[N - 1];
                i = 1;
            }
            k -= 1;
        }
        self.mt[0] = 0x8000_0000;
        self.index = N;
    }

    fn genrand_u32(&mut self) -> u32 {
        if self.index >= N {
            for kk in 0..N {
                let y = (self.mt[kk] & 0x8000_0000) | (self.mt[(kk + 1) % N] & 0x7fff_ffff);
                let mag = if y & 1 == 1 { 0x9908_b0df } else { 0 };
                self.mt[kk] = self.mt[(kk + M) % N] ^ (y >> 1) ^ mag;
            }
            self.index = 0;
        }
        let mut y = self.mt[self.index];
        self.index += 1;
        y ^= y >> 11;
        y ^= (y << 7) & 0x9d2c_5680;
        y ^= (y << 15) & 0xefc6_0000;
        y ^= y >> 18;
        y
    }

    /// `random.random()`: a float in [0, 1) with 53 random bits.
    pub fn random(&mut self) -> f64 {
        let a = (self.genrand_u32() >> 5) as f64;
        let b = (self.genrand_u32() >> 6) as f64;
        (a * 67_108_864.0 + b) * (1.0 / 9_007_199_254_740_992.0)
    }

    /// `random.gauss(mu, sigma)`: Box-Muller, caching the second deviate.
    pub fn gauss(&mut self, mu: f64, sigma: f64) -> f64 {
        let z = match self.gauss_next.take() {
            Some(z) => z,
            None => {
                let two_pi = std::f64::consts::TAU;
                let x2pi = self.random() * two_pi;
                let g2rad = (-2.0 * (1.0 - self.random()).ln()).sqrt();
                self.gauss_next = Some(x2pi.sin() * g2rad);
                x2pi.cos() * g2rad
            }
        };
        mu + z * sigma
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seeded_stream_matches_cpython() {
        // CPython 3.12: random.Random(2026).random() twice, then gauss(0, 1) x3.
        let mut rng = PyRandom::new(2026);
        assert_eq!(rng.random(), 0.11911988496396309);
        assert_eq!(rng.random(), 0.5025157552312506);
        let mut rng = PyRandom::new(2026);
        assert_eq!(rng.gauss(0.0, 1.0), 0.8658723979019295);
        assert_eq!(rng.gauss(0.0, 1.0), 0.804144125101057);
        assert_eq!(rng.gauss(0.0, 1.0), -1.977517460705795);
    }
}

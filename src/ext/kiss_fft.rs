//! KISS FFT — Complex FFT (floating-point, scalar)
//!
//! Port of Mark Borgerding's KISS FFT (`src/ext/kiss_fft.c` +
//! `_kiss_fft_guts.h`).  BSD-3-Clause licensed.
//!
//! Simplified for TIC-80's use case:
//! - `f32` scalar only (no fixed-point, no SIMD)
//! - Heap-allocated plan
//! - Safe Rust

use std::f64::consts::PI as PI_64;

// ---------------------------------------------------------------------------
// Complex number type
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Complex {
    pub r: f32,
    pub i: f32,
}

impl Complex {
    #[inline(always)]
    pub fn new(r: f32, i: f32) -> Self {
        Complex { r, i }
    }
}

// inline helpers matching the KISS FFT C macros
#[inline(always)]
pub fn c_add(a: Complex, b: Complex) -> Complex {
    Complex::new(a.r + b.r, a.i + b.i)
}
#[inline(always)]
pub fn c_sub(a: Complex, b: Complex) -> Complex {
    Complex::new(a.r - b.r, a.i - b.i)
}
#[inline(always)]
pub fn c_mul(a: Complex, b: Complex) -> Complex {
    Complex::new(a.r * b.r - a.i * b.i, a.r * b.i + a.i * b.r)
}
#[inline(always)]
pub fn c_scale(a: Complex, s: f32) -> Complex {
    Complex::new(a.r * s, a.i * s)
}
#[inline(always)]
fn half(x: f32) -> f32 {
    x * 0.5
}

// ---------------------------------------------------------------------------
// FFT plan
// ---------------------------------------------------------------------------

pub struct Fft {
    nfft: usize,
    inverse: bool,
    factors: Vec<(i32, i32)>, // (radix p, next_n m)
    twiddles: Vec<Complex>,
}

impl Fft {
    /// Allocate and plan an FFT of length `nfft`.
    ///
    /// `inverse`: `false` = forward FFT, `true` = inverse FFT.
    pub fn new(nfft: usize, inverse: bool) -> Self {
        // Pre-compute twiddle factors
        let mut twiddles = Vec::with_capacity(nfft);
        for i in 0..nfft {
            let phase = -2.0 * PI_64 * (i as f64) / (nfft as f64);
            let phase = if inverse { -phase } else { phase } as f32;
            twiddles.push(Complex::new(phase.cos(), phase.sin()));
        }

        // Factor decomposition
        let factors = factor(nfft);

        Fft {
            nfft,
            inverse,
            factors,
            twiddles,
        }
    }

    /// Length of the FFT.
    pub fn len(&self) -> usize {
        self.nfft
    }

    /// Whether this is an inverse FFT.
    pub fn inverse(&self) -> bool {
        self.inverse
    }

    /// Perform an out-of-place FFT: transform `input` into `output`.
    ///
    /// Both slices must have length `self.nfft`.
    /// `input` and `output` must not overlap.
    pub fn transform(&self, input: &[Complex], output: &mut [Complex]) {
        assert_eq!(input.len(), self.nfft);
        assert_eq!(output.len(), self.nfft);

        kf_work(
            output, input, 0, 1, 1, &self.factors, &self.twiddles, self.nfft, self.inverse,
        );
    }

    /// Perform an in-place FFT by copying into an internal temporary buffer.
    pub fn transform_in_place(&self, buf: &mut [Complex]) {
        assert_eq!(buf.len(), self.nfft);
        let mut tmp = vec![Complex::default(); self.nfft];
        kf_work(
            &mut tmp, buf, 0, 1, 1, &self.factors, &self.twiddles, self.nfft, self.inverse,
        );
        buf.copy_from_slice(&tmp);
    }

    /// Convenience: forward FFT, returning a new vector.
    pub fn forward(&self, input: &[Complex]) -> Vec<Complex> {
        let mut out = vec![Complex::default(); self.nfft];
        self.transform(input, &mut out);
        out
    }

    /// Find the smallest `k >= n` with only factors 2, 3, 5.
    pub fn next_fast_size(n: usize) -> usize {
        let mut n = n;
        loop {
            let mut m = n;
            while m % 2 == 0 {
                m /= 2;
            }
            while m % 3 == 0 {
                m /= 3;
            }
            while m % 5 == 0 {
                m /= 5;
            }
            if m <= 1 {
                return n;
            }
            n += 1;
        }
    }
}

// ---------------------------------------------------------------------------
// Factor decomposition
// ---------------------------------------------------------------------------

/// Decompose `n` into factors `(p, m)` such that `p * m = n` (iteratively).
fn factor(n: usize) -> Vec<(i32, i32)> {
    let mut n = n as i32;
    let mut factors = Vec::new();
    let floor_sqrt = (n as f64).sqrt().floor() as i32;
    let mut p = 4;

    loop {
        // Find the next factor
        while n % p != 0 {
            p = match p {
                4 => 2,
                2 => 3,
                _ => p + 2,
            };
            if p > floor_sqrt {
                p = n;
            }
        }
        n /= p;
        factors.push((p, n));
        if n <= 1 {
            break;
        }
    }
    factors
}

// ---------------------------------------------------------------------------
// Butterflies
// ---------------------------------------------------------------------------

/// radix-2 butterfly
fn bfly2(fout: &mut [Complex], fstride: usize, twiddles: &[Complex], m: usize) {
    for i in 0..m {
        let tw = twiddles[i * fstride];
        let t = c_mul(fout[i + m], tw);
        let f0 = fout[i];
        fout[i + m] = c_sub(f0, t);
        fout[i] = c_add(f0, t);
    }
}

/// radix-4 butterfly
fn bfly4(fout: &mut [Complex], fstride: usize, twiddles: &[Complex], m: usize, inverse: bool) {
    let m2 = 2 * m;
    let m3 = 3 * m;
    for i in 0..m {
        let tw1 = twiddles[i * fstride];
        let tw2 = twiddles[i * fstride * 2];
        let tw3 = twiddles[i * fstride * 3];

        let s0 = c_mul(fout[i + m], tw1);
        let s1 = c_mul(fout[i + m2], tw2);
        let s2 = c_mul(fout[i + m3], tw3);

        let f0 = fout[i];
        let s5 = c_sub(f0, s1);
        let f0_new = c_add(f0, s1);
        let s3 = c_add(s0, s2);
        let s4 = c_sub(s0, s2);

        fout[i] = c_add(f0_new, s3);
        fout[i + m2] = c_sub(f0_new, s3);

        if inverse {
            fout[i + m] = Complex::new(s5.r - s4.i, s5.i + s4.r);
            fout[i + m3] = Complex::new(s5.r + s4.i, s5.i - s4.r);
        } else {
            fout[i + m] = Complex::new(s5.r + s4.i, s5.i - s4.r);
            fout[i + m3] = Complex::new(s5.r - s4.i, s5.i + s4.r);
        }
    }
}

/// radix-3 butterfly
fn bfly3(fout: &mut [Complex], fstride: usize, twiddles: &[Complex], m: usize) {
    let m2 = 2 * m;
    let epi3_i = twiddles[fstride * m].i; // sin component only

    for i in 0..m {
        let tw1 = twiddles[i * fstride];
        let tw2 = twiddles[i * fstride * 2];

        let s1 = c_mul(fout[i + m], tw1);
        let s2 = c_mul(fout[i + m2], tw2);

        let s3 = c_add(s1, s2);
        let s0 = c_sub(s1, s2);

        let f0 = fout[i];
        let new_r = f0.r - half(s3.r);
        let new_i = f0.i - half(s3.i);

        fout[i] = c_add(f0, s3);

        let sc = c_scale(s0, epi3_i);
        // C: Fout[m2] = Fout[m]_intermediate + scratch[0].i, — scratch[0].r
        fout[i + m2] = Complex::new(new_r + sc.i, new_i - sc.r);
        // C: Fout[m]  = Fout[m]_intermediate - scratch[0].i, + scratch[0].r
        fout[i + m] = Complex::new(new_r - sc.i, new_i + sc.r);
    }
}

/// radix-5 butterfly
#[allow(clippy::too_many_arguments)]
fn bfly5(fout: &mut [Complex], fstride: usize, twiddles: &[Complex], m: usize) {
    let ya = twiddles[fstride * m];
    let yb = twiddles[fstride * 2 * m];

    for u in 0..m {
        let s0 = fout[u];

        let s1 = c_mul(fout[u + m], twiddles[u * fstride]);
        let s2 = c_mul(fout[u + 2 * m], twiddles[2 * u * fstride]);
        let s3 = c_mul(fout[u + 3 * m], twiddles[3 * u * fstride]);
        let s4 = c_mul(fout[u + 4 * m], twiddles[4 * u * fstride]);

        let s7 = c_add(s1, s4);
        let s10 = c_sub(s1, s4);
        let s8 = c_add(s2, s3);
        let s9 = c_sub(s2, s3);

        fout[u] = Complex::new(s0.r + s7.r + s8.r, s0.i + s7.i + s8.i);

        let s5 = Complex::new(
            s0.r + s7.r * ya.r + s8.r * yb.r,
            s0.i + s7.i * ya.r + s8.i * yb.r,
        );
        let s6 = Complex::new(
            s10.i * ya.i + s9.i * yb.i,
            -(s10.r * ya.i) - s9.r * yb.i,
        );

        fout[u + m] = c_sub(s5, s6);
        fout[u + 4 * m] = c_add(s5, s6);

        let s11 = Complex::new(
            s0.r + s7.r * yb.r + s8.r * ya.r,
            s0.i + s7.i * yb.r + s8.i * ya.r,
        );
        let s12 = Complex::new(
            -(s10.i * yb.i) + s9.i * ya.i,
            s10.r * yb.i - s9.r * ya.i,
        );

        fout[u + 2 * m] = c_add(s11, s12);
        fout[u + 3 * m] = c_sub(s11, s12);
    }
}

/// Generic radix-p butterfly
#[allow(clippy::too_many_arguments)]
fn bfly_generic(
    fout: &mut [Complex],
    fstride: usize,
    twiddles: &[Complex],
    m: usize,
    p: usize,
    nfft: usize,
) {
    let mut scratch = vec![Complex::default(); p];

    for u in 0..m {
        // Gather
        for q1 in 0..p {
            scratch[q1] = fout[u + q1 * m];
        }

        // Scatter/recombine
        for q1 in 0..p {
            let k = u + q1 * m;
            fout[k] = scratch[0];
            for q in 1..p {
                let twidx = (fstride * k) % nfft;
                let t = c_mul(scratch[q], twiddles[twidx]);
                fout[k] = c_add(fout[k], t);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Core work function (recursive)
// ---------------------------------------------------------------------------

fn kf_work(
    fout: &mut [Complex],
    f: &[Complex],
    f_offset: usize,
    fstride: usize,
    in_stride: usize,
    factors: &[(i32, i32)],
    twiddles: &[Complex],
    nfft: usize,
    inverse: bool,
) {
    if factors.is_empty() {
        return;
    }

    let (p_i32, m_i32) = factors[0];
    let p = p_i32 as usize;
    let m = m_i32 as usize;
    let rest = &factors[1..];

    if m == 1 {
        // Base case: copy strided input elements
        let mut off = f_offset;
        for i in 0..p {
            fout[i] = f[off];
            off += fstride * in_stride;
        }
    } else {
        // Recurse: p DFTs of size m, advancing f_offset by fstride*in_stride each time
        let mut off = f_offset;
        for k in 0..p {
            let start = k * m;
            kf_work(
                &mut fout[start..start + m],
                f,
                off,
                fstride * p,
                in_stride,
                rest,
                twiddles,
                nfft,
                inverse,
            );
            off += fstride * in_stride;
        }
    }

    // Recombine the p smaller DFTs
    match p {
        2 => bfly2(fout, fstride, twiddles, m),
        3 => bfly3(fout, fstride, twiddles, m),
        4 => bfly4(fout, fstride, twiddles, m, inverse),
        5 => bfly5(fout, fstride, twiddles, m),
        _ => bfly_generic(fout, fstride, twiddles, m, p, nfft),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: compare complex vectors with tolerance
    fn approx_eq(a: &[Complex], b: &[Complex], eps: f32) {
        assert_eq!(a.len(), b.len(), "length mismatch");
        for (i, (x, y)) in a.iter().zip(b).enumerate() {
            let d = ((x.r - y.r).powi(2) + (x.i - y.i).powi(2)).sqrt();
            assert!(
                d < eps,
                "mismatch at [{}]: ({}, {}) vs ({}, {}), diff={}",
                i, x.r, x.i, y.r, y.i, d
            );
        }
    }

    /// Delta impulse → all frequency bins have magnitude 1
    #[test]
    fn delta_impulse_forward() {
        let n = 64;
        let fft = Fft::new(n, false);
        let mut input = vec![Complex::default(); n];
        input[0] = Complex::new(1.0, 0.0);
        let output = fft.forward(&input);

        for (k, &x) in output.iter().enumerate() {
            let mag = (x.r.powi(2) + x.i.powi(2)).sqrt();
            assert!(
                (mag - 1.0).abs() < 1e-5,
                "bin [{}]: magnitude {}, expected 1",
                k, mag
            );
        }
    }

    /// Constant signal → only DC bin is non-zero
    #[test]
    fn constant_signal() {
        let n = 16;
        let fft = Fft::new(n, false);
        let input = vec![Complex::new(1.0, 0.0); n];
        let output = fft.forward(&input);

        assert!((output[0].r - 16.0).abs() < 1e-5, "DC real = {}", output[0].r);
        assert!(output[0].i.abs() < 1e-5, "DC imag = {}", output[0].i);

        for k in 1..n {
            assert!(
                output[k].r.abs() < 1e-5 && output[k].i.abs() < 1e-5,
                "bin [{k}] should be zero: ({}, {})",
                output[k].r, output[k].i
            );
        }
    }

    /// Forward + inverse = identity (round-trip)
    #[test]
    fn round_trip() {
        let n = 32;
        let fft_fwd = Fft::new(n, false);
        let fft_inv = Fft::new(n, true);

        let input: Vec<Complex> = (0..n)
            .map(|i| Complex::new((i as f32 * 0.3).sin(), (i as f32 * 0.7).cos()))
            .collect();

        let freq = fft_fwd.forward(&input);
        let mut output = vec![Complex::default(); n];
        fft_inv.transform(&freq, &mut output);

        // Inverse FFT divides by n
        for x in &mut output {
            x.r /= n as f32;
            x.i /= n as f32;
        }
        approx_eq(&input, &output, 1e-5);
    }

    /// Sine wave → peak at bin k
    #[test]
    fn sine_wave_peak() {
        let n = 128;
        let fft = Fft::new(n, false);
        let k = 3usize;

        let input: Vec<Complex> = (0..n)
            .map(|i| {
                let phase = 2.0 * std::f32::consts::PI * (k as f32) * (i as f32) / (n as f32);
                Complex::new(phase.sin(), 0.0)
            })
            .collect();

        let output = fft.forward(&input);
        let peak_mag = |idx: usize| (output[idx].r.powi(2) + output[idx].i.powi(2)).sqrt();
        let max_peak = peak_mag(k);
        let expected_peak = (n as f32) / 2.0;

        assert!(max_peak > expected_peak * 0.9,
            "peak at bin {}: magnitude {}, expected ~{}", k, max_peak, expected_peak);

        for i in 0..n {
            if i != k && i != n - k {
                let mag = peak_mag(i);
                assert!(mag < 1.0, "bin [{}] should be near zero, got {}", i, mag);
            }
        }
    }

    /// In-place transform
    #[test]
    fn in_place() {
        let n = 16;
        let fft = Fft::new(n, false);
        let mut buf: Vec<Complex> = (0..n)
            .map(|i| Complex::new(i as f32, 0.0))
            .collect();

        let expected = fft.forward(&buf);
        fft.transform_in_place(&mut buf);

        approx_eq(&expected, &buf, 1e-6);
    }

    /// next_fast_size
    #[test]
    fn test_next_fast_size() {
        assert_eq!(Fft::next_fast_size(1), 1);
        assert_eq!(Fft::next_fast_size(2), 2);
        assert_eq!(Fft::next_fast_size(6), 6);
        assert_eq!(Fft::next_fast_size(7), 8);
        assert_eq!(Fft::next_fast_size(13), 15);
        assert_eq!(Fft::next_fast_size(14), 15);
        assert_eq!(Fft::next_fast_size(17), 18);
        assert_eq!(Fft::next_fast_size(31), 32);
        assert_eq!(Fft::next_fast_size(128), 128);
        assert_eq!(Fft::next_fast_size(211), 216);
    }

    /// Power-of-two sizes
    #[test]
    fn power_of_two() {
        for bits in 1..=10 {
            let n = 1 << bits;
            let fft = Fft::new(n, false);
            let mut input = vec![Complex::default(); n];
            for i in 0..n {
                input[i] = Complex::new((i as f32 * 0.13).cos(), (i as f32 * 0.17).sin());
            }
            // Just check it runs without panicking and isn't all zeros
            let output = fft.forward(&input);
            let has_energy = output.iter().any(|x| x.r.abs() > 1e-6 || x.i.abs() > 1e-6);
            assert!(has_energy, "size {}: output is all zeros", n);
        }
    }

    /// Non-power-of-two sizes (3*5, 2*3*5, etc.)
    #[test]
    fn mixed_radix() {
        for &n in &[6, 15, 30, 60, 90, 120, 150, 180] {
            let fft = Fft::new(n, false);
            let input = vec![Complex::new(1.0, 0.0); n];
            let output = fft.forward(&input);
            // DC should be n
            assert!((output[0].r - n as f32).abs() < 1e-4,
                "size {}: DC bin = {}, expected {}", n, output[0].r, n);
        }
    }

    /// Round-trip for non-power-of-two
    #[test]
    fn round_trip_mixed_radix() {
        for &n in &[6, 15, 30, 60] {
            let fft_fwd = Fft::new(n, false);
            let fft_inv = Fft::new(n, true);
            let input: Vec<Complex> = (0..n)
                .map(|i| Complex::new((i as f32).sin(), (i as f32 * 2.0).cos()))
                .collect();

            let freq = fft_fwd.forward(&input);
            let mut output = vec![Complex::default(); n];
            fft_inv.transform(&freq, &mut output);
            for x in &mut output {
                x.r /= n as f32;
                x.i /= n as f32;
            }
            approx_eq(&input, &output, 1e-5);
        }
    }
}

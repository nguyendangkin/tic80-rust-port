//! KISS FFT — Real-optimised FFT
//!
//! Port of Mark Borgerding's `kiss_fftr.c` (BSD-3-Clause).
//! Uses the complex FFT from `kiss_fft` to compute a real-input FFT ~45% faster.
//!
//! A real FFT of length N is computed as a complex FFT of length N/2
//! plus O(N) post-processing via "super-twiddle" factors.

#[path = "kiss_fft.rs"]
mod kiss_fft;

use kiss_fft::{c_add, c_mul, c_sub, Complex, Fft};

// ---------------------------------------------------------------------------
// Real FFT plan
// ---------------------------------------------------------------------------

pub struct RealFft {
    nfft: usize,       // original real FFT length (must be even)
    ncfft: usize,      // complex FFT length = nfft / 2
    sub_fft: Fft,      // underlying complex FFT
    tmpbuf: Vec<Complex>,
    super_twiddles: Vec<Complex>,
}

impl RealFft {
    /// Allocate a real FFT plan for length `nfft` (must be even).
    ///
    /// `inverse`: `false` = forward real-FFT, `true` = inverse real-FFT.
    pub fn new(nfft: usize, inverse: bool) -> Self {
        assert!(nfft % 2 == 0, "Real FFT length must be even, got {}", nfft);

        let ncfft = nfft / 2;
        let sub_fft = Fft::new(ncfft, inverse);

        // Pre-compute super-twiddle factors
        let mut super_twiddles = Vec::with_capacity(ncfft / 2);
        for i in 0..ncfft / 2 {
            let phase = -std::f64::consts::PI * ((i + 1) as f64 / ncfft as f64 + 0.5);
            let phase = if inverse { -phase } else { phase } as f32;
            super_twiddles.push(Complex::new(phase.cos(), phase.sin()));
        }

        // Temporary buffer for the complex FFT output
        let tmpbuf = vec![Complex::default(); ncfft];

        RealFft {
            nfft,
            ncfft,
            sub_fft,
            tmpbuf,
            super_twiddles,
        }
    }

    /// Forward real FFT.
    ///
    /// `input`: `nfft` real (float) samples.
    /// `output`: `nfft/2 + 1` complex frequency bins.
    pub fn forward(&mut self, input: &[f32], output: &mut [Complex]) {
        assert_eq!(input.len(), self.nfft, "input length mismatch");
        assert_eq!(
            output.len(),
            self.ncfft + 1,
            "output should have nfft/2+1 = {} elements",
            self.ncfft + 1
        );
        assert!(!self.sub_fft.inverse(), "forward RealFft called with inverse plan");

        // Pack real input as complex: even samples → real, odd → imag
        // then perform complex FFT of length ncfft
        let packed: &[Complex] = unsafe {
            // SAFETY: input has nfft = ncfft*2 floats, which has the same
            // memory layout as ncfft Complex values (r,i pairs)
            std::slice::from_raw_parts(input.as_ptr() as *const Complex, self.ncfft)
        };
        self.sub_fft.transform(packed, &mut self.tmpbuf);

        // DC & Nyquist bins
        // C_FIXDIV(tdc,2) is a NO-OP for float; raw sums are used
        let tdc = self.tmpbuf[0];
        output[0] = Complex::new(tdc.r + tdc.i, 0.0);
        output[self.ncfft] = Complex::new(tdc.r - tdc.i, 0.0);

        // Intermediate bins
        for k in 1..=self.ncfft / 2 {
            let fpk = self.tmpbuf[k];
            let fpnk = Complex::new(
                self.tmpbuf[self.ncfft - k].r,
                -self.tmpbuf[self.ncfft - k].i,
            );
            // C_FIXDIV is NO-OP for float — no divide by 2 here

            let f1k = c_add(fpk, fpnk);
            let f2k = c_sub(fpk, fpnk);

            let tw = c_mul(f2k, self.super_twiddles[k - 1]);

            // HALF_OF applied only at the final output stage
            output[k] = Complex::new(
                0.5 * (f1k.r + tw.r),
                0.5 * (f1k.i + tw.i),
            );
            output[self.ncfft - k] = Complex::new(
                0.5 * (f1k.r - tw.r),
                0.5 * (tw.i - f1k.i),
            );
        }
    }

    /// Inverse real FFT.
    ///
    /// `input`: `nfft/2 + 1` complex frequency bins.
    /// `output`: `nfft` real (float) samples.
    pub fn inverse(&mut self, input: &[Complex], output: &mut [f32]) {
        assert_eq!(input.len(), self.ncfft + 1, "input length mismatch");
        assert_eq!(output.len(), self.nfft, "output length mismatch");
        assert!(self.sub_fft.inverse(), "inverse RealFft called with forward plan");

        // Pack into complex buffer
        // C_FIXDIV is NO-OP for float — use raw sums
        self.tmpbuf[0] = Complex::new(
            input[0].r + input[self.ncfft].r,
            input[0].r - input[self.ncfft].r,
        );

        for k in 1..=self.ncfft / 2 {
            let fk = input[k];
            let fnkc = Complex::new(
                input[self.ncfft - k].r,
                -input[self.ncfft - k].i,
            );

            // C_FIXDIV is NO-OP for float — no divide by 2
            let fek = c_add(fk, fnkc);
            let tmp = c_sub(fk, fnkc);
            let fok = c_mul(tmp, self.super_twiddles[k - 1]);

            self.tmpbuf[k] = c_add(fek, fok);
            self.tmpbuf[self.ncfft - k] = Complex::new(
                fek.r - fok.r,
                -(fek.i - fok.i), // note: C code does *= -1 on .i
            );
        }

        // Complex IFFT
        let out_complex: &mut [Complex] = unsafe {
            std::slice::from_raw_parts_mut(output.as_mut_ptr() as *mut Complex, self.ncfft)
        };
        self.sub_fft.transform(&self.tmpbuf, out_complex);
    }

    /// Length of the real FFT.
    pub fn len(&self) -> usize {
        self.nfft
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: compare float vectors with tolerance
    fn approx_eq_f32(a: &[f32], b: &[f32], eps: f32) {
        assert_eq!(a.len(), b.len());
        for (i, (x, y)) in a.iter().zip(b).enumerate() {
            let d = (x - y).abs();
            assert!(d < eps, "mismatch at [{}]: {} vs {}", i, x, y);
        }
    }

    /// Delta impulse → all frequency bins equal
    #[test]
    fn delta_impulse() {
        let n = 32;
        let mut rfft = RealFft::new(n, false);

        let mut input = vec![0.0f32; n];
        input[0] = 1.0;

        let mut output = vec![Complex::default(); n / 2 + 1];
        rfft.forward(&input, &mut output);

        // All bins should have magnitude 1.0
        for (k, &x) in output.iter().enumerate() {
            let mag = (x.r.powi(2) + x.i.powi(2)).sqrt();
            assert!(
                (mag - 1.0).abs() < 1e-5,
                "bin [{}]: magnitude {}, expected 1", k, mag
            );
        }
    }

    /// Constant signal → only DC bin non-zero
    #[test]
    fn constant_signal() {
        let n = 16;
        let mut rfft = RealFft::new(n, false);
        let input = vec![1.0f32; n];
        let mut output = vec![Complex::default(); n / 2 + 1];
        rfft.forward(&input, &mut output);

        // DC = sum = n
        assert!((output[0].r - n as f32).abs() < 1e-5, "DC = {}", output[0].r);
        assert!(output[0].i.abs() < 1e-5, "DC imag = {}", output[0].i);
        // Nyquist = 0 (sum of alternating (+1, -1, +1, -1...) = 0 for n even)
        assert!(output[n / 2].r.abs() < 1e-5, "Nyquist real = {}", output[n / 2].r);

        for k in 1..n / 2 {
            assert!(
                output[k].r.abs() < 1e-5 && output[k].i.abs() < 1e-5,
                "bin [{}] should be zero: ({}, {})",
                k, output[k].r, output[k].i
            );
        }
    }

    /// Round-trip: forward real FFT → inverse → original signal
    #[test]
    fn round_trip() {
        for &n in &[8, 16, 32, 64, 128] {
            let mut rfft_fwd = RealFft::new(n, false);
            let mut rfft_inv = RealFft::new(n, true);

            let input: Vec<f32> = (0..n)
                .map(|i| (i as f32 * 0.7).sin() + (i as f32 * 0.3).cos())
                .collect();

            let mut freq = vec![Complex::default(); n / 2 + 1];
            rfft_fwd.forward(&input, &mut freq);

            let mut output = vec![0.0f32; n];
            rfft_inv.inverse(&freq, &mut output);

            // IFFT output is not divided by n — C kiss_fftri doesn't
            // Normalize our expected output
            for x in &mut output {
                *x /= n as f32;
            }

            approx_eq_f32(&input, &output, 1e-4);
        }
    }

    /// Non-power-of-two even sizes
    #[test]
    fn non_power_of_two() {
        for &n in &[6, 10, 12, 30, 60] {
            let mut rfft_fwd = RealFft::new(n, false);
            let mut rfft_inv = RealFft::new(n, true);

            let input: Vec<f32> = (0..n).map(|i| i as f32).collect();

            let mut freq = vec![Complex::default(); n / 2 + 1];
            rfft_fwd.forward(&input, &mut freq);

            let mut output = vec![0.0f32; n];
            rfft_inv.inverse(&freq, &mut output);
            for x in &mut output {
                *x /= n as f32;
            }

            approx_eq_f32(&input, &output, 1e-4);
        }
    }

    /// Match against known values from C kiss_fftr
    #[test]
    fn known_values() {
        let n = 8;
        let mut rfft = RealFft::new(n, false);
        let input = vec![1.0f32, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0];
        let mut output = vec![Complex::default(); n / 2 + 1];
        rfft.forward(&input, &mut output);

        // DC bin = sum = 36
        assert!((output[0].r - 36.0).abs() < 1e-5, "DC real = {}", output[0].r);

        // Nyquist bin = sum of alternating signs = (1-2+3-4+5-6+7-8) = -4
        assert!((output[n / 2].r - (-4.0)).abs() < 1e-5, "Nyquist real = {}", output[n / 2].r);
    }
}

//! Causal EEG preprocessing primitives for embedded use.
//!
//! The embedded target does not need to design filters at runtime. Instead, the
//! host can export second-order-section coefficients and firmware can apply the
//! resulting causal SOS cascade sample-by-sample.

use core::array;

/// One second-order section using transposed direct-form II state.
#[derive(Clone, Copy, Debug)]
pub struct SosSection {
    b: [f32; 3],
    a: [f32; 3],
    z1: f32,
    z2: f32,
}

impl SosSection {
    /// Creates a section from normalized IIR coefficients.
    ///
    /// `a0` is expected to be 1.0, matching SciPy SOS output.
    pub const fn new(b: [f32; 3], a: [f32; 3]) -> Self {
        Self { b, a, z1: 0.0, z2: 0.0 }
    }

    /// Creates a section from a SciPy-style SOS row:
    /// `[b0, b1, b2, a0, a1, a2]`.
    pub const fn from_scipy_row(row: [f32; 6]) -> Self {
        Self::new([row[0], row[1], row[2]], [row[3], row[4], row[5]])
    }

    /// Resets the internal delay state.
    pub fn reset(&mut self) {
        self.z1 = 0.0;
        self.z2 = 0.0;
    }

    /// Filters a single sample.
    #[inline(always)]
    pub fn process_sample(&mut self, input: f32) -> f32 {
        let output = self.b[0] * input + self.z1;
        self.z1 = self.b[1] * input - self.a[1] * output + self.z2;
        self.z2 = self.b[2] * input - self.a[2] * output;
        output
    }
}

/// A causal cascade of second-order sections.
#[derive(Clone, Copy, Debug)]
pub struct SosCascade<const SECTIONS: usize> {
    sections: [SosSection; SECTIONS],
}

impl<const SECTIONS: usize> SosCascade<SECTIONS> {
    /// Creates a new cascade from fixed SOS sections.
    pub const fn new(sections: [SosSection; SECTIONS]) -> Self {
        Self { sections }
    }

    /// Creates a cascade from SciPy-style SOS rows.
    pub const fn from_scipy_rows(rows: [[f32; 6]; SECTIONS]) -> Self {
        let mut sections =
            [SosSection::from_scipy_row([0.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
                SECTIONS];
        let mut idx = 0;
        while idx < SECTIONS {
            sections[idx] = SosSection::from_scipy_row(rows[idx]);
            idx += 1;
        }
        Self::new(sections)
    }

    /// Returns a copy of the configured sections.
    pub const fn sections(&self) -> [SosSection; SECTIONS] {
        self.sections
    }

    /// Resets all section states.
    pub fn reset(&mut self) {
        let mut idx = 0;
        while idx < SECTIONS {
            self.sections[idx].reset();
            idx += 1;
        }
    }

    /// Filters a single sample through the full cascade.
    #[inline(always)]
    pub fn process_sample(&mut self, input: f32) -> f32 {
        let mut value = input;
        let mut idx = 0;
        while idx < SECTIONS {
            value = self.sections[idx].process_sample(value);
            idx += 1;
        }
        value
    }
}

/// Per-channel causal preprocessing with a shared SOS cascade shape.
#[derive(Clone, Copy, Debug)]
pub struct ChannelPreprocessor<const CHANNELS: usize, const SECTIONS: usize> {
    cascades: [SosCascade<SECTIONS>; CHANNELS],
}

impl<const CHANNELS: usize, const SECTIONS: usize>
    ChannelPreprocessor<CHANNELS, SECTIONS>
{
    /// Creates a preprocessor with per-channel cascades.
    pub const fn new(cascades: [SosCascade<SECTIONS>; CHANNELS]) -> Self {
        Self { cascades }
    }

    /// Creates a preprocessor where all channels share the same SOS cascade.
    pub fn shared(cascade: SosCascade<SECTIONS>) -> Self {
        Self::new(array::from_fn(|_| cascade))
    }

    /// Resets all per-channel states.
    pub fn reset(&mut self) {
        let mut idx = 0;
        while idx < CHANNELS {
            self.cascades[idx].reset();
            idx += 1;
        }
    }

    /// Filters a single frame of `f32` samples.
    #[inline(always)]
    pub fn process_frame(
        &mut self,
        frame: [f32; CHANNELS],
    ) -> [f32; CHANNELS] {
        let mut out = [0.0; CHANNELS];
        let mut idx = 0;
        while idx < CHANNELS {
            out[idx] = self.cascades[idx].process_sample(frame[idx]);
            idx += 1;
        }
        out
    }

    /// Filters a single frame of integer ADC samples.
    ///
    /// `scale` converts ADC counts into the floating-point domain used by the
    /// exported coefficients. For example, if host-side calibration used volts,
    /// `scale` should map counts to volts.
    #[inline(always)]
    pub fn process_frame_i32(
        &mut self,
        frame: [i32; CHANNELS],
        scale: f32,
    ) -> [f32; CHANNELS] {
        let mut out = [0.0; CHANNELS];
        let mut idx = 0;
        while idx < CHANNELS {
            out[idx] =
                self.cascades[idx].process_sample(frame[idx] as f32 * scale);
            idx += 1;
        }
        out
    }
}

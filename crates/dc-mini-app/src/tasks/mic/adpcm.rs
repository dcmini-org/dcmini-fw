use audio_codec_algorithms::{encode_adpcm_ima, AdpcmImaState};

pub(crate) struct AdpcmEncoder {
    state: AdpcmImaState,
}

#[allow(dead_code)]
impl AdpcmEncoder {
    pub fn new() -> Self {
        Self { state: AdpcmImaState::new() }
    }

    /// Snapshot the current decoder state so the host can decode this packet independently.
    pub fn decoder_state(&self) -> (i32, u32) {
        (self.state.predictor as i32, self.state.step_index as u32)
    }

    /// Encode a block of PCM samples into packed IMA ADPCM nibbles.
    /// `out` must be at least `pcm.len() / 2` bytes long.
    /// Two consecutive 4-bit codes are packed per byte (first sample in low nibble).
    pub fn encode_block(&mut self, pcm: &[i16], out: &mut [u8]) {
        for (i, chunk) in pcm.chunks_exact(2).enumerate() {
            let lo = encode_adpcm_ima(chunk[0], &mut self.state);
            let hi = encode_adpcm_ima(chunk[1], &mut self.state);
            out[i] = (hi << 4) | (lo & 0x0F);
        }
    }

    pub fn reset(&mut self) {
        self.state = AdpcmImaState::new();
    }
}

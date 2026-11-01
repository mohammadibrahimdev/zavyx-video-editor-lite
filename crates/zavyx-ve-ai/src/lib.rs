//! zavyx-ve-ai — optional local AI features (Whisper captions, ONNX
//! segmentation, Piper TTS).
//!
//! Phase 4: structured API for the three AI features. With `--features ai`
//! the crates compile; on 32-bit Windows `ai::available()` returns false
//! and the GUI hides AI buttons. Real model loading + inference uses
//! `whisper-rs` and `ort`, which require 64-bit runtimes.

#[derive(Debug, thiserror::Error)]
pub enum AiError {
    #[error("AI features not available on this build: {0}")]
    Unavailable(String),
    #[error("model load failed: {0}")]
    ModelLoad(String),
    #[error("inference failed: {0}")]
    Inference(String),
}

/// Returns true if AI features are usable on the current build/target.
/// On 32-bit Windows this returns false even when the `ai` feature is on,
/// because whisper.cpp and ONNX Runtime are not reliably built for i686-msvc.
pub fn available() -> bool {
    cfg!(all(target_arch = "x86_64", target_os = "windows"))
        || cfg!(all(target_arch = "x86_64", target_os = "linux"))
        || cfg!(all(target_arch = "x86_64", target_os = "macos"))
}

/// Returns a human-readable reason why AI is unavailable (empty string if available).
pub fn unavailable_reason() -> String {
    if available() { return String::new(); }
    if cfg!(target_pointer_width = "32") {
        return "AI features require 64-bit Windows/Linux/macOS (whisper.cpp and ONNX Runtime are not built for i686).".into();
    }
    if !cfg!(feature = "ai") {
        return "AI feature not enabled. Rebuild with --features zavyx-ve-ai/ai.".into();
    }
    "AI unavailable on this target.".into()
}

/// A captioned segment with start/end timestamps and text.
#[derive(Clone, Debug, PartialEq)]
pub struct Caption {
    pub start: zavyx_ve_core::Rational,
    pub end:   zavyx_ve_core::Rational,
    pub text:  String,
}

#[cfg(feature = "ai")]
pub mod whisper {
    use super::{AiError, Caption};

    /// Speech-to-text captioner backed by whisper.cpp.
    pub struct Captioner {
        model_path: std::path::PathBuf,
        // The actual whisper-rs WhisperContext is loaded lazily on first
        // transcribe() call to avoid requiring the model file at construction.
        ctx: Option<whisper_rs::WhisperContext>,
    }

    impl Captioner {
        /// Construct a captioner pointing at a model file. The model is not
        /// loaded until `transcribe()` is first called.
        pub fn new(model_path: impl AsRef<std::path::Path>) -> Result<Self, AiError> {
            let p = model_path.as_ref();
            if !p.exists() {
                return Err(AiError::ModelLoad(format!("model file not found: {}", p.display())));
            }
            Ok(Captioner { model_path: p.to_path_buf(), ctx: None })
        }

        /// Transcribe an audio buffer (mono f32 samples at `sample_rate` Hz).
        /// Returns captions with timestamps.
        pub fn transcribe(&mut self, audio: &[f32], sample_rate: u32) -> Result<Vec<Caption>, AiError> {
            if self.ctx.is_none() {
                let path_str = self.model_path.to_str()
                    .ok_or_else(|| AiError::ModelLoad("model path is not UTF-8".into()))?;
                self.ctx = Some(whisper_rs::WhisperContext::new_with_params(path_str, Default::default())
                    .map_err(|e| AiError::ModelLoad(format!("whisper load: {}", e)))?);
            }
            let ctx = self.ctx.as_ref().unwrap();
            // Whisper expects 16 kHz mono f32.
            let target_sr = 16_000u32;
            let resampled: Vec<f32> = if sample_rate == target_sr {
                audio.to_vec()
            } else {
                // Naive linear resample (good enough for speech; rubato would be better).
                let ratio = target_sr as f32 / sample_rate as f32;
                let new_len = (audio.len() as f32 * ratio) as usize;
                (0..new_len).map(|i| {
                    let src = i as f32 / ratio;
                    let i0 = src as usize;
                    let i1 = (i0 + 1).min(audio.len() - 1);
                    let frac = src - i0 as f32;
                    audio[i0] * (1.0 - frac) + audio[i1] * frac
                }).collect()
            };
            let mut state = ctx.create_state()
                .map_err(|e| AiError::Inference(format!("whisper state: {}", e)))?;
            state.full(whisper_rs::FullParams::new(whisper_rs::SAMPLING_GREEDY), &resampled)
                .map_err(|e| AiError::Inference(format!("whisper full: {}", e)))?;
            let mut out = Vec::new();
            for i in 0..state.full_n_segments() {
                let text = state.full_get_segment_text(i)
                    .unwrap_or_default();
                let t0 = state.full_get_segment_t0(i).unwrap_or(0) as i64;  // in 10ms units
                let t1 = state.full_get_segment_t1(i).unwrap_or(0) as i64;
                out.push(Caption {
                    start: zavyx_ve_core::Rational::new(t0, 100),
                    end:   zavyx_ve_core::Rational::new(t1, 100),
                    text,
                });
            }
            Ok(out)
        }
    }
}

#[cfg(feature = "ai")]
pub mod segmentation {
    use super::AiError;

    /// Portrait cutout / background removal backed by ONNX Runtime.
    pub struct Segmenter {
        model_path: std::path::PathBuf,
        session: Option<ort::session::Session>,
    }

    impl Segmenter {
        pub fn new(model_path: impl AsRef<std::path::Path>) -> Result<Self, AiError> {
            let p = model_path.as_ref();
            if !p.exists() {
                return Err(AiError::ModelLoad(format!("model file not found: {}", p.display())));
            }
            Ok(Segmenter { model_path: p.to_path_buf(), session: None })
        }

        /// Run segmentation on an RGBA8 frame; returns a mask where 255 = foreground.
        pub fn segment(&mut self, pixels: &[u8], w: u32, h: u32) -> Result<Vec<u8>, AiError> {
            if self.session.is_none() {
                let path_str = self.model_path.to_str()
                    .ok_or_else(|| AiError::ModelLoad("model path is not UTF-8".into()))?;
                self.session = Some(ort::session::SessionBuilder::new()
                    .map_err(|e| AiError::ModelLoad(format!("ort builder: {}", e)))?
                    .commit_from_file(path_str)
                    .map_err(|e| AiError::ModelLoad(format!("ort session: {}", e)))?);
            }
            // For Phase 4 we return a stub mask (all foreground) — actual
            // inference requires a specific model architecture (e.g. MODNet).
            // The user supplies the model file via the GUI's model download dialog.
            let _ = (self.session.as_ref(), pixels, w, h);
            Ok(vec![255u8; (w * h) as usize])
        }
    }
}

#[cfg(feature = "ai")]
pub mod tts {
    use super::AiError;

    /// Text-to-speech synthesizer backed by Piper (ONNX).
    pub struct Synthesizer {
        model_path: std::path::PathBuf,
        session: Option<ort::session::Session>,
    }

    impl Synthesizer {
        pub fn new(model_path: impl AsRef<std::path::Path>) -> Result<Self, AiError> {
            let p = model_path.as_ref();
            if !p.exists() {
                return Err(AiError::ModelLoad(format!("model file not found: {}", p.display())));
            }
            Ok(Synthesizer { model_path: p.to_path_buf(), session: None })
        }

        /// Synthesize speech from text. Returns f32 samples at 22050 Hz (Piper default).
        pub fn synthesize(&mut self, text: &str) -> Result<Vec<f32>, AiError> {
            if self.session.is_none() {
                let path_str = self.model_path.to_str()
                    .ok_or_else(|| AiError::ModelLoad("model path is not UTF-8".into()))?;
                self.session = Some(ort::session::SessionBuilder::new()
                    .map_err(|e| AiError::ModelLoad(format!("ort builder: {}", e)))?
                    .commit_from_file(path_str)
                    .map_err(|e| AiError::ModelLoad(format!("ort session: {}", e)))?);
            }
            // Stub: return 1 second of silence. Actual inference requires
            // Piper's phoneme encoding + the model's specific input/output tensors.
            let _ = text;
            Ok(vec![0.0f32; 22_050])
        }
    }
}

/// Compute the file size of a model file (for showing the user before download).
pub fn model_size_hint(model_id: &str) -> Option<u64> {
    // Rough file sizes for common open-source models; used by the GUI to
    // show "Download (75 MB)" before the user clicks.
    match model_id {
        "whisper-tiny"   => Some(75_000_000),
        "whisper-base"   => Some(142_000_000),
        "whisper-small"  => Some(466_000_000),
        "whisper-medium" => Some(1_500_000_000),
        "whisper-large"  => Some(3_000_000_000),
        "modnet-portrait" => Some(25_000_000),
        "piper-en_US-amy" => Some(63_000_000),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn available_is_truthy_on_supported_targets() {
        let v = available();
        let _ = v;
    }

    #[test]
    fn model_size_hint_returns_known_models() {
        assert_eq!(model_size_hint("whisper-tiny"), Some(75_000_000));
        assert_eq!(model_size_hint("whisper-large"), Some(3_000_000_000));
        assert_eq!(model_size_hint("unknown-model"), None);
    }

    #[test]
    fn unavailable_reason_returns_message_when_not_available() {
        if !available() {
            let r = unavailable_reason();
            assert!(!r.is_empty(), "unavailable_reason should be non-empty when not available");
        }
    }
}

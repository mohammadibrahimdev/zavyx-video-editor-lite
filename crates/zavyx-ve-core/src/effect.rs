//! The `Effect` and `Transition` traits.
//!
//! Effects operate on a single clip's frames. Transitions blend two clips'
//! frames. Both have typed, keyframeable parameters declared via a
//! `ParamSchema`, so new effects/transitions can be added without touching
//! the engine.

use crate::id::EffectId;
use crate::time::Rational;
use crate::keyframe::KeyframeTrack;
use serde::{Serialize, Deserialize};

/// The type of a single parameter exposed by an effect.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ParamType {
    /// Float in [min, max] with a step.
    Float { min: f64, max: f64, step: f64, default: f64 },
    /// Integer in [min, max].
    Int   { min: i64, max: i64, default: i64 },
    /// Boolean toggle.
    Bool  { default: bool },
    /// One-of-N choice.
    Choice { options: Vec<String>, default: String },
    /// RGB color (0-1 per channel).
    Color { default: [f32; 3] },
    /// 2D point (x, y) in clip-local space.
    Point2D { default: [f64; 2] },
    /// Free text (e.g. a title's words).
    Text { default: String },
}

/// Schema describing an effect's parameters. Drives the GUI widget generation.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ParamSchema {
    pub params: Vec<ParamDef>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ParamDef {
    pub id: String,
    pub label: String,
    pub ty: ParamType,
    /// Whether this parameter can be keyframed.
    pub keyframeable: bool,
}

/// The runtime value of a parameter — either a constant or a keyframe track.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ParamValue {
    Const(f64),
    Keyframed(KeyframeTrack<f64>),
    /// Text value; reads as 0.0 when used as a number.
    Text(String),
}

impl ParamValue {
    pub fn at(&self, time: Rational) -> f64 {
        match self {
            ParamValue::Const(v) => *v,
            ParamValue::Keyframed(t) => t.value_at_f64(time),
            ParamValue::Text(_) => 0.0,
        }
    }
    pub fn text(&self) -> Option<&str> {
        match self {
            ParamValue::Text(s) => Some(s),
            _ => None,
        }
    }
}

/// An instantiation of an effect on a clip, with bound parameter values.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EffectInstance {
    pub id: EffectId,
    /// The effect's stable identifier (e.g. "zavyx.blur.gaussian").
    pub effect_id: String,
    pub enabled: bool,
    pub params: std::collections::BTreeMap<String, ParamValue>,
}

impl EffectInstance {
    pub fn new(effect_id: impl Into<String>) -> Self {
        EffectInstance {
            id: EffectId::new(),
            effect_id: effect_id.into(),
            enabled: true,
            params: std::collections::BTreeMap::new(),
        }
    }
    pub fn param(&self, name: &str, time: Rational) -> Option<f64> {
        self.params.get(name).map(|v| v.at(time))
    }
    pub fn set_const(&mut self, name: impl Into<String>, value: f64) {
        self.params.insert(name.into(), ParamValue::Const(value));
    }
    pub fn set_text(&mut self, name: impl Into<String>, value: impl Into<String>) {
        self.params.insert(name.into(), ParamValue::Text(value.into()));
    }
    pub fn text(&self, name: &str) -> Option<&str> {
        self.params.get(name).and_then(ParamValue::text)
    }
}

/// Frame view passed to an effect's process function. The renderer fills in
/// the actual pixel data; the effect just reads/writes the buffer.
///
/// In Phase 1 this is a CPU RGBA8 view; Phase 3+ will add a GPU/wgpu variant.
#[derive(Debug)]
pub struct FrameView<'a> {
    pub width:  u32,
    pub height: u32,
    /// RGBA8, row-major, top-to-bottom. Stride = width * 4.
    pub pixels: &'a mut [u8],
    /// Time on the timeline at this frame.
    pub time: Rational,
}

/// Immutable frame view, used as the `to` input to a transition.
#[derive(Debug, Copy, Clone)]
pub struct FrameViewRef<'a> {
    pub width:  u32,
    pub height: u32,
    pub pixels: &'a [u8],
    pub time:   Rational,
}

impl<'a> FrameViewRef<'a> {
    pub fn from_view(v: &'a FrameView<'a>) -> Self {
        FrameViewRef {
            width: v.width,
            height: v.height,
            pixels: v.pixels,
            time: v.time,
        }
    }
}

/// Render context passed to effects. Holds shared resources (shader cache,
/// GPU device, thread pool). In Phase 1 it is empty.
pub struct RenderCtx<'a> {
    pub working_color_space: &'a str,
}

/// The Effect trait. Implementations live in `zavyx-ve-render`.
pub trait Effect: Send + Sync {
    fn id(&self) -> &str;
    fn name(&self) -> &str;
    fn param_schema(&self) -> ParamSchema;
    fn process(&self, frame: &mut FrameView<'_>, params: &std::collections::BTreeMap<String, ParamValue>, ctx: &RenderCtx<'_>);
}

/// The Transition trait. Blends `from` and `to` frames by `alpha` in [0,1].
pub trait Transition: Send + Sync {
    fn id(&self) -> &str;
    fn name(&self) -> &str;
    fn param_schema(&self) -> ParamSchema;
    fn process(&self, from: &mut FrameView<'_>, to: FrameViewRef<'_>, alpha: f64,
               params: &std::collections::BTreeMap<String, ParamValue>,
               ctx: &RenderCtx<'_>);
}

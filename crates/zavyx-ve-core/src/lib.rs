//! # zavyx-ve-core
//!
//! Pure-Rust project model for the Zavyx Video Editor.
//!
//! This crate has **no I/O and no platform dependencies** — it owns the
//! project's data structures, the undo/redo command pattern, keyframe
//! interpolation, the effect trait, and serde-based serialization with
//! versioned migration. Every public type is `Send + Sync` and works
//! identically on `i686` and `x86_64` Windows (and on Linux/macOS for
//! testing).
//!
//! ## Module map
//!
//! | Module      | Responsibility                                            |
//! |-------------|----------------------------------------------------------|
//! | [`time`]    | Rational time, frame-rate maths, `TimeRange`             |
//! | [`id`]      | Strongly-typed IDs (SeqId, TrackId, ClipId, …)            |
//! | [`project`] | `Project`, `MediaPool`, `Settings`                       |
//! | [`timeline`]| `Sequence`, `Track`, `Block`, edit algorithms            |
//! | [`command`] | `Command` trait + `History` (undo/redo)                   |
//! | [`keyframe`]| `Keyframe<T>`, `Easing`, interpolation                    |
//! | [`effect`]  | `Effect`/`Transition` traits, parameter schema           |
//! | [`serde`]   | Save/load `.zvep`, schema migration v1 → v2              |

pub mod time;
pub mod id;
pub mod project;
pub mod timeline;
pub mod command;
pub mod keyframe;
pub mod effect;
pub mod edits;
pub mod snapping;
pub mod shortcuts;
pub mod serde;

pub use time::{Rational, TimeRange, FrameRate};
pub use id::*;
pub use project::{Project, MediaPool, Settings, MediaRef, MediaInfo, AspectRatio};
pub use timeline::{Sequence, Track, TrackKind, Block, ClipBlock, GapBlock};
pub use command::{Command, History, HistoryAction};
pub use keyframe::{Keyframe, Easing, KeyframeTrack};
pub use effect::{Effect, EffectInstance, Transition, ParamSchema, ParamType, ParamValue, FrameView, FrameViewRef};

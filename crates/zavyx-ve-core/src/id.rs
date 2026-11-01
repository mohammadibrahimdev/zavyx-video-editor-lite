//! Strongly-typed IDs for every entity in the project model.
//!
//! Using newtypes (rather than raw `u64` or `String`) means the compiler
//! rejects mixing a `TrackId` with a `ClipId`. IDs are stable across saves
//! (they are part of the `.zvep` format), so we use a tagged enum rather than
//! pointer addresses.

use serde::{Serialize, Deserialize};
use uuid::Uuid;

/// Macro to declare a new ID newtype backed by `Uuid`.
macro_rules! id_newtype {
    ($name:ident, $prefix:literal) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
        pub struct $name(pub Uuid);

        impl $name {
            pub fn new() -> Self { Self(Uuid::new_v4()) }
            pub fn as_uuid(&self) -> &Uuid { &self.0 }
            pub fn prefix() -> &'static str { $prefix }
        }

        impl Default for $name {
            fn default() -> Self { Self::new() }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "{}:{}", $prefix, self.0.simple())
            }
        }

        impl std::str::FromStr for $name {
            type Err = String;
            fn from_str(s: &str) -> Result<Self, Self::Err> {
                let (prefix, rest) = s.split_once(':')
                    .ok_or_else(|| format!("missing prefix separator in {}", s))?;
                if prefix != $prefix {
                    return Err(format!("expected prefix {} got {}", $prefix, prefix));
                }
                let u = Uuid::parse_str(rest).map_err(|e| e.to_string())?;
                Ok(Self(u))
            }
        }
    };
}

id_newtype!(ProjectId,  "proj");
id_newtype!(SeqId,      "seq");
id_newtype!(TrackId,    "trk");
id_newtype!(ClipId,     "clip");
id_newtype!(MediaId,    "media");
id_newtype!(EffectId,   "fx");
id_newtype!(MarkerId,   "mrk");
id_newtype!(TransitionId,"trn");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_round_trip() {
        let id = ClipId::new();
        let s = id.to_string();
        let back: ClipId = s.parse().unwrap();
        assert_eq!(id, back);
    }

    #[test]
    fn ids_dont_mix() {
        // Compile-time check that two ID types don't compare:
        // this line would fail to compile if uncommented:
        // let _ = ClipId::new() == TrackId::new();
    }
}

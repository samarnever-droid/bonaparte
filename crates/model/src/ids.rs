//! Strongly-typed identifiers. Newtypes so IDs cannot be mixed up (RULES §2.5:
//! make invalid states unrepresentable).

use serde::{Deserialize, Serialize};

macro_rules! define_id {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub u64);

        impl $name {
            /// The next sequential ID. Allocators live on `Project`.
            pub fn next(self) -> Self {
                Self(self.0 + 1)
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, concat!(stringify!($name), "({})"), self.0)
            }
        }
    };
}

define_id!(
    /// A composition (a timeline of layers that renders to one video output).
    CompId
);
define_id!(
    /// A layer within a composition.
    LayerId
);
define_id!(
    /// An imported media asset (image, video, or audio) in the project library.
    MediaId
);

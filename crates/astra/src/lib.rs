//! # Astra — the workspace storage engine.
//!
//! Projects outgrow RAM. Astra stores media as **content-addressed
//! chunks** on disk (SHA-256 addressed, deduplicated, mmap-read) behind
//! an **Atra hot cache**, so imports stream instead of buffer, project
//! documents carry hash references instead of megabytes of base64, and
//! the workspace bound is the disk — not the heap.
//!
//! Layout:
//! ```text
//! <root>/chunks/<xx>/<hash>   immutable chunk files (raw bytes)
//! ```
//! Reads go chunk → Atra (hot) → mmap. `gc(keep)` reclaims every chunk
//! not named in `keep`. All hashes are hex SHA-256 of the chunk bytes.
pub mod store;

pub use store::{AstraStats, Store, CHUNK_SIZE, STORE_VERSION};

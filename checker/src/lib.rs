//! plinth-check: the independent checker (design dev-loop.md section 4.4, PRD NFR-6).
//!
//! It shares no code or types with the generator. Skeleton stage: its own .chitubox reader
//! (`chitubox`), a second structural walk (`layout`), its own STL reader (`stl`), a `dump`
//! command and a JSON `report` in which every hard-limit rule is "unverified" until built.

pub mod bytes;
pub mod chitubox;
pub mod dump;
pub mod json;
pub mod layout;
pub mod limits;
pub mod loop_canary;
pub mod report;
pub mod sha256;
pub mod status;
pub mod stl;

//! plinth-check: the independent checker (design dev-loop.md section 4.4, PRD NFR-6).
//!
//! It shares no code or types with the generator. Its own .chitubox reader (`chitubox`), a
//! second structural walk (`layout`), its own STL reader (`stl`), a `dump` command, the
//! rule measurements (`measure` and the modules it runs) and a JSON `report`. A rule it
//! cannot measure is "unverified", never "pass".

pub mod audit;
pub mod bytes;
pub mod canon;
pub mod chains;
pub mod chitubox;
pub mod departure;
pub mod dump;
pub mod gap;
pub mod h12;
pub mod h3;
pub mod h4;
pub mod h5;
pub mod h6;
pub mod h7;
pub mod h9;
pub mod hollow;
pub mod islands;
pub mod json;
pub mod layout;
pub mod limits;
pub mod loop_canary;
pub mod measure;
pub mod mesh;
pub mod outcome;
pub mod par;
pub mod parts;
pub mod patch;
pub mod poly;
pub mod report;
pub mod scene;
pub mod sha256;
pub mod solid;
pub mod status;
pub mod stl;
pub mod testkit;
pub mod tri;
pub mod v3;

#[cfg(test)]
mod validate;

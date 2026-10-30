//! v0.50: time-explicit, display-independent stellar astrometry.
//! Mathematical approximations and limits are documented in docs/STELLAR_ENGINE_050.md.
mod types;
mod time;
mod catalogue;
mod astrometry;
mod observer;
mod engine;
pub use types::{Vec3, Observer, ObservationOptions, EarthOrientation, ProperMotionRa, EquatorialFrame, EpochTimeScale, StarRecord, PreparedCatalogue, StarResult, HorizontalDirection, SnapshotMetadata, QualityFlags, StellarError};
pub use time::UtcInstant;
pub use catalogue::{embedded_reference_stars, prepare_catalogue};
pub use observer::{prepare_observation, ObservationContext};
pub use engine::{evaluate_stars,evaluate_icrs_direction};
/// Phase-2 production Tycho-2 reader. Headless, independent of the reference adapter.
pub mod production;
/// v0.53 offline HIP -> stable Tycho-ID raw-material index.
pub mod hip_index;
/// v0.53.2 deterministic Stellarium culture loader and Tycho binding layer.
pub mod culture;

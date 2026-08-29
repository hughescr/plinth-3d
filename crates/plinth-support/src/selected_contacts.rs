//! The S1 hand-off, in its own module so that its fields are genuinely private.
//!
//! [`SelectedContacts`] seals its fingerprint at birth under the SEALED-INPUT
//! RULE ([`crate::digest_seal`]).  That seal is only as strong as the privacy
//! of the fields it folds: a writer that can build the struct literal, or
//! reach a field, can leave the sealed word describing contents that are no
//! longer there.
//!
//! This type used to live in the crate root.  A field with no `pub` is visible
//! to the module that declares it AND to every descendant module -- and every
//! module in a crate descends from the crate root, so "private" there meant
//! "visible to the whole crate".  Any module could have written the literal
//! and bypassed [`SelectedContacts::new`].  Here the fields are private to
//! this module alone, which has no writer other than the constructor, so the
//! doc-comment on the seal is now enforced by the compiler rather than by
//! convention.

use crate::digest_seal::SealedDigest;
use crate::{CoverageSample, SelectedContact};

/// Immutable S1 hand-off.  This is a placement plan, not a scene or a run API.
#[derive(Clone, Debug, PartialEq)]
pub struct SelectedContacts {
    contacts: Vec<SelectedContact>,
    coverage_samples: Vec<CoverageSample>,
    /// The hand-off's own fingerprint, sealed at birth under the SEALED-INPUT
    /// RULE ([`crate::digest_seal`]).  Both fields above are private, this type
    /// has no `&mut self` method, and the hand-off is REPLACED rather than
    /// edited when selection re-runs — so the walk that produced this word can
    /// never have a different answer for this value.  `Clone` carries it
    /// because a clone carries every field it folds.
    digest: SealedDigest,
}

impl SelectedContacts {
    /// The only constructor.  It seals the fingerprint from the very vectors
    /// it is taking ownership of, so a hand-off is never observable without a
    /// true seal.
    #[must_use]
    pub(crate) fn new(
        contacts: Vec<SelectedContact>,
        coverage_samples: Vec<CoverageSample>,
    ) -> Self {
        let digest = SealedDigest::seal(selected_contacts_digest(&contacts, &coverage_samples));
        Self {
            contacts,
            coverage_samples,
            digest,
        }
    }

    #[must_use]
    pub fn contacts(&self) -> &[SelectedContact] {
        &self.contacts
    }

    #[must_use]
    pub fn coverage_samples(&self) -> &[CoverageSample] {
        &self.coverage_samples
    }

    /// Stable FNV-1a digest; selection itself makes no new native query.
    ///
    /// Served from the seal taken at construction.  On `raking_blade` this word
    /// was re-derived about twenty-six times per repair trial over 16,640
    /// coverage samples against seventeen contacts, so over 99% of its bytes
    /// were the invariant sample array.
    #[must_use]
    pub fn digest(&self) -> u64 {
        self.digest.get()
    }

    /// Build a hand-off whose seal deliberately DISAGREES with its contents.
    ///
    /// Test-only, and named to be unmissable in a diff.  This is the one way to
    /// produce the state the SEALED-INPUT RULE exists to make impossible, and
    /// it exists so that the checks which must CATCH that state can be tested
    /// at all.  Production has no path to it: the fields are private to this
    /// module, and [`Self::new`] — which seals from the vectors it is taking —
    /// is the only other constructor.
    ///
    /// Before this type moved into its own module its fields were reachable
    /// from every module in the crate, so a test could forge this state by
    /// writing the field directly.  That is what made the seal a convention
    /// rather than a guarantee.  The forging affordance is kept, because the
    /// sabotage test that uses it is real and load-bearing; what changed is
    /// that it is now the ONLY door, it is `#[cfg(test)]`, and it says what it
    /// is.
    #[cfg(test)]
    #[must_use]
    pub(crate) fn forged_with_stale_seal(
        mut self,
        tamper: impl FnOnce(&mut [SelectedContact]),
    ) -> Self {
        tamper(&mut self.contacts);
        self
    }

    /// The unmemoized fingerprint walk, retained verbatim as the oracle for
    /// the seal.  Test-only: production code has no reason to pay for it,
    /// because nothing can make the seal disagree with it.
    #[cfg(test)]
    #[must_use]
    pub(crate) fn digest_full_walk(&self) -> u64 {
        selected_contacts_digest(&self.contacts, &self.coverage_samples)
    }
}

/// The selected-contact fingerprint walk.
///
/// A free function on the pieces rather than a method, so that the SEALED-INPUT
/// RULE's "seal at birth" obligation ([`crate::digest_seal`]) can be met: the
/// word is folded from the very vectors being placed into the
/// [`SelectedContacts`] literal, before the hand-off exists.
fn selected_contacts_digest(
    contacts: &[SelectedContact],
    coverage_samples: &[CoverageSample],
) -> u64 {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut state = OFFSET;
    let mut add = |bytes: &[u8]| {
        for &byte in bytes {
            state ^= u64::from(byte);
            state = state.wrapping_mul(PRIME);
        }
    };
    for contact in contacts {
        add(&contact.island_id.to_le_bytes());
        add(&contact.candidate_id.0.to_le_bytes());
        add(&contact.served_face_id.to_le_bytes());
        for value in contact
            .point_mm
            .iter()
            .chain(contact.normal.iter())
            .chain([contact.delicacy, contact.supported_area_mm2].iter())
        {
            add(&value.to_bits().to_le_bytes());
        }
    }
    for sample in coverage_samples {
        add(&sample.island_id.to_le_bytes());
        add(&sample.face_id.to_le_bytes());
        add(&sample.component_id.to_le_bytes());
        for value in sample.point_mm.iter().chain([sample.area_mm2].iter()) {
            add(&value.to_bits().to_le_bytes());
        }
    }
    state
}

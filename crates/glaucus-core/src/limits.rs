// SPDX-FileCopyrightText: Glaucus contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Resource limits for safe YAML processing.
//!
//! Provides configurable limits to protect against adversarial inputs
//! such as billion laughs attacks, deep nesting, and memory exhaustion.
//!
//! # Defaults
//!
//! All limits have safe defaults. Users must explicitly opt in to raise them.
//!
//! ```
//! use glaucus_core::limits::ResourceLimits;
//!
//! let limits = ResourceLimits::default();
//! assert_eq!(limits.max_depth, 128);
//! ```

/// The deepest nesting composition can survive, whatever `max_depth` says.
///
/// Composition is recursive: one stack frame per nesting level. `max_depth` is
/// therefore not a pure resource knob — set high enough, it stops bounding memory
/// and starts deciding whether the process survives at all. A stack overflow
/// **aborts**; it cannot be caught, so a library consumer loses their process
/// rather than receiving an error.
///
/// This ceiling exists so that cannot happen. The effective limit is
/// `min(max_depth, MAX_SAFE_DEPTH)` — see
/// [`effective_max_depth`](ResourceLimits::effective_max_depth) — so raising
/// `max_depth`, or using [`unlimited`](ResourceLimits::unlimited), yields a clean
/// [`DepthLimitExceeded`](crate::error::ErrorKind::DepthLimitExceeded) rather
/// than an abort.
///
/// # Why 192
///
/// Measured, not guessed — and the measurement has a 24x spread, which is the
/// whole reason this number is small:
///
/// | build | stack | overflows at |
/// | ----- | ----- | ------------ |
/// | `opt-level = 1` | 8 MiB (main thread) | ~7,300 |
/// | `opt-level = 1` | 2 MiB (spawned thread) | ~1,850 |
/// | **`opt-level = 0`** | **2 MiB (spawned thread)** | **~300** |
///
/// Stack frame size scales with optimisation level, so "how deep can we recurse"
/// is not one number. A ceiling sized against the roomy case is not a ceiling.
///
/// The binding case is not hypothetical: a consumer building glaucus inside their
/// own debug profile, on a runtime whose workers get Rust's default 2 MiB thread
/// stack, lands exactly there. 192 keeps roughly a 1.5x margin against it.
///
/// The default `max_depth` of 128 sits comfortably inside this, so ordinary use
/// is unaffected; documents nested past 192 are pathological rather than merely
/// large.
pub const MAX_SAFE_DEPTH: usize = 192;

/// The shallowest overflow measured across realistic builds: `opt-level = 0` on
/// the 2 MiB stack Rust gives a spawned thread. Recorded as a constant so the
/// margin below is checked by the compiler rather than trusted.
const WORST_MEASURED_OVERFLOW_DEPTH: usize = 300;

// Compile-time, not a test: raising `MAX_SAFE_DEPTH` without re-measuring should
// fail the BUILD, not wait for someone to run the suite -- the failure mode it
// guards against is an uncatchable abort in a consumer's process.
const _: () = assert!(
    MAX_SAFE_DEPTH * 3 / 2 <= WORST_MEASURED_OVERFLOW_DEPTH,
    "MAX_SAFE_DEPTH leaves under a 1.5x margin against the worst measured \
     overflow depth; re-measure on a 2 MiB stack at opt-level 0 before raising it"
);

/// Configurable resource limits for YAML processing.
///
/// These limits protect against known attack vectors:
/// - **Billion Laughs**: Exponential alias expansion → [`max_alias_expansions`](Self::max_alias_expansions)
/// - **Deep nesting**: Stack overflow → [`max_depth`](Self::max_depth)
/// - **Huge documents**: Memory exhaustion → [`max_document_size`](Self::max_document_size)
/// - **Huge keys**: Memory exhaustion → [`max_key_length`](Self::max_key_length)
/// - **Node flood**: CPU exhaustion → [`max_node_count`](Self::max_node_count)
/// - **Alias amplification**: memory exhaustion → [`max_total_alias_nodes`](Self::max_total_alias_nodes)
/// - **Anchor flood**: memory exhaustion → [`max_anchors`](Self::max_anchors)
/// - **Huge anchor names**: memory exhaustion → [`max_anchor_name_length`](Self::max_anchor_name_length)
/// - **Huge scalars**: memory exhaustion → [`max_scalar_length`](Self::max_scalar_length)
#[derive(Debug, Clone, PartialEq, Eq)]
#[must_use]
pub struct ResourceLimits {
    /// Maximum nesting depth for collections. Default: 128.
    pub max_depth: usize,
    /// Maximum number of alias expansions allowed. Default: 1,024.
    pub max_alias_expansions: usize,
    /// Maximum document size in bytes. Default: 256 MiB.
    pub max_document_size: usize,
    /// Maximum length of a mapping key in bytes. Default: 1,024.
    pub max_key_length: usize,
    /// Maximum number of nodes in the representation graph. Default: 1,000,000.
    pub max_node_count: usize,
    /// Maximum cumulative nodes materialised by alias expansion. Default: 100,000.
    ///
    /// This is deliberately **not** the same quantity as
    /// [`max_node_count`](Self::max_node_count), which counts parser *events* —
    /// that is, the size of the source document. An alias resolves by cloning the
    /// anchored subtree, so a document whose event count is trivially small can
    /// still materialise an arbitrarily large tree. Counting events cannot observe
    /// that growth, because the cloned nodes were never parsed.
    ///
    /// This limit counts the other quantity: the total nodes conjured by alias
    /// expansion across a document. It is what bounds the classic billion-laughs
    /// amplification, where source size grows linearly while the materialised tree
    /// grows exponentially.
    ///
    /// The default is conservative on purpose. Materialising 100,000 nodes through
    /// anchors is already orders of magnitude beyond any real configuration file,
    /// and this module's contract is that defaults are safe and callers opt in
    /// explicitly to raise them.
    pub max_total_alias_nodes: usize,
    /// Maximum number of distinct anchors in one document. Default: 1,024.
    ///
    /// Every anchor is retained for the whole document so a later alias can find
    /// it, so the anchor map is a per-document allocation an author controls
    /// directly. Bounding the count bounds that map.
    pub max_anchors: usize,
    /// Maximum length of an anchor name in bytes. Default: 1,024.
    ///
    /// [`max_key_length`](Self::max_key_length) bounds mapping *keys* and does not
    /// reach anchor names, which are a separate attacker-controlled string that
    /// gets owned and used as a map key.
    pub max_anchor_name_length: usize,
    /// Maximum byte length of any single scalar. Default: 10 MiB.
    ///
    /// [`max_document_size`](Self::max_document_size) bounds the whole document
    /// and leaves a single scalar inside it unbounded up to that size, which is
    /// 256 MiB by default — far more than any legitimate individual value.
    ///
    /// A companion knob exists as `ParserPolicies::max_scalar_length`, and the two
    /// are not redundant: this one is the *safe ceiling* every caller gets for
    /// free, while the policy is opt-in *further tightening*. The effective bound
    /// is the smaller of the two, so a policy can never raise this ceiling.
    pub max_scalar_length: usize,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            max_depth: 128,
            max_alias_expansions: 1_024,
            max_document_size: 256 * 1024 * 1024, // 256 MiB
            max_key_length: 1_024,
            max_node_count: 1_000_000,
            max_total_alias_nodes: 100_000,
            max_anchors: 1_024,
            max_anchor_name_length: 1_024,
            max_scalar_length: 10 * 1024 * 1024, // 10 MiB
        }
    }
}

impl ResourceLimits {
    /// The nesting depth actually enforced: `min(max_depth, MAX_SAFE_DEPTH)`.
    ///
    /// [`max_depth`](Self::max_depth) is what the caller asked for;
    /// this is what can be delivered without risking a stack overflow during
    /// composition. See [`MAX_SAFE_DEPTH`] for why the two differ.
    #[must_use]
    pub const fn effective_max_depth(&self) -> usize {
        if self.max_depth < MAX_SAFE_DEPTH {
            self.max_depth
        } else {
            MAX_SAFE_DEPTH
        }
    }

    /// Creates limits with no restrictions. Use with caution — only for trusted inputs.
    ///
    /// Nesting depth is the one exception and cannot be uncapped: it is still
    /// clamped to [`MAX_SAFE_DEPTH`], because exceeding it aborts the process
    /// rather than returning an error. "Trusted" input can still be deep.
    pub const fn unlimited() -> Self {
        Self {
            max_depth: usize::MAX,
            max_alias_expansions: usize::MAX,
            max_document_size: usize::MAX,
            max_key_length: usize::MAX,
            max_node_count: usize::MAX,
            max_total_alias_nodes: usize::MAX,
            max_anchors: usize::MAX,
            max_anchor_name_length: usize::MAX,
            max_scalar_length: usize::MAX,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_limits_are_safe() {
        let limits = ResourceLimits::default();
        assert_eq!(limits.max_depth, 128);
        assert_eq!(limits.max_alias_expansions, 1_024);
        assert_eq!(limits.max_document_size, 256 * 1024 * 1024);
        assert_eq!(limits.max_key_length, 1_024);
        assert_eq!(limits.max_node_count, 1_000_000);
        assert_eq!(limits.max_total_alias_nodes, 100_000);
        assert_eq!(limits.max_anchors, 1_024);
        assert_eq!(limits.max_anchor_name_length, 1_024);
        assert_eq!(limits.max_scalar_length, 10 * 1024 * 1024);
    }

    #[test]
    fn unlimited_is_unrestricted() {
        let limits = ResourceLimits::unlimited();
        assert_eq!(limits.max_depth, usize::MAX);
        assert_eq!(
            limits.effective_max_depth(),
            MAX_SAFE_DEPTH,
            "unlimited() must not uncap depth: exceeding it aborts, it does not error"
        );
        assert_eq!(limits.max_alias_expansions, usize::MAX);
        assert_eq!(limits.max_total_alias_nodes, usize::MAX);
        assert_eq!(limits.max_anchors, usize::MAX);
        assert_eq!(limits.max_anchor_name_length, usize::MAX);
        assert_eq!(limits.max_scalar_length, usize::MAX);
    }
}

#[cfg(test)]
mod safe_depth_tests {
    use super::{MAX_SAFE_DEPTH, ResourceLimits};

    #[test]
    fn effective_depth_is_the_lower_of_the_two() {
        let mut l = ResourceLimits::default();
        assert_eq!(l.effective_max_depth(), 128, "default is below the ceiling");

        l.max_depth = 10;
        assert_eq!(l.effective_max_depth(), 10);

        l.max_depth = MAX_SAFE_DEPTH;
        assert_eq!(l.effective_max_depth(), MAX_SAFE_DEPTH);

        l.max_depth = MAX_SAFE_DEPTH + 1;
        assert_eq!(
            l.effective_max_depth(),
            MAX_SAFE_DEPTH,
            "raising max_depth past the ceiling must not raise the ceiling"
        );

        l.max_depth = usize::MAX;
        assert_eq!(l.effective_max_depth(), MAX_SAFE_DEPTH);
    }

    #[test]
    fn ceiling_is_above_the_default_so_ordinary_use_is_unaffected() {
        assert!(
            ResourceLimits::default().max_depth < MAX_SAFE_DEPTH,
            "the ceiling must not constrain the default configuration"
        );
    }
}

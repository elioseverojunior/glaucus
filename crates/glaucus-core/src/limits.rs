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
    /// Creates limits with no restrictions. Use with caution — only for trusted inputs.
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
        assert_eq!(limits.max_alias_expansions, usize::MAX);
        assert_eq!(limits.max_total_alias_nodes, usize::MAX);
        assert_eq!(limits.max_anchors, usize::MAX);
        assert_eq!(limits.max_anchor_name_length, usize::MAX);
        assert_eq!(limits.max_scalar_length, usize::MAX);
    }
}

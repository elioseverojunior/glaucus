// SPDX-FileCopyrightText: Glaucus contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! A replayable parser-event tape, for resolving aliases without a node tree.
//!
//! The tree path resolves an alias by cloning an already-built `Node`. A
//! streaming path has no nodes, so it must replay the **events** that built the
//! anchored value instead. That difference is the whole reason this type exists.

use std::collections::HashMap;
use std::ops::Range;

use glaucus_core::error::{Error, ErrorKind, ParserConfig, Result};
use glaucus_core::limits::ResourceLimits;
use glaucus_core::parser::Parser;
use glaucus_core::parser::event::{Event, EventKind};
use glaucus_core::types::Span;

/// Returns the anchor an event declares, if any.
///
/// Only three event kinds can carry one. Reading it here — rather than making the
/// caller hand it back — is what lets [`Tape::next`] decide whether an event needs
/// recording *before* it is returned, which is what keeps the anchor-free path
/// allocation-free.
fn declared_anchor<'e>(event: &'e Event<'_>) -> Option<&'e str> {
    match &event.kind {
        EventKind::Scalar { anchor, .. }
        | EventKind::SequenceStart { anchor, .. }
        | EventKind::MappingStart { anchor, .. } => anchor.as_deref(),
        _ => None,
    }
}

/// Parser events, with the ability to replay an anchored span.
pub(crate) struct Tape<'a> {
    parser: Parser<'a>,
    /// Every recorded event, in one flat buffer.
    ///
    /// **Not a `Vec` per anchor.** Nested anchors — `&x {p: &y 1}` — then cost
    /// nothing extra, because `y`'s range sits *inside* `x`'s and the inner
    /// events are stored once. Per-anchor vectors would store them twice, and the
    /// duplication compounds with nesting depth.
    buffer: Vec<Event<'a>>,
    /// Anchor name to its half-open range in `buffer`.
    anchors: HashMap<String, Range<usize>>,
    /// Anchors whose value is still being read, innermost last: name, where the
    /// span starts, and the nesting depth the anchor was declared at.
    ///
    /// Recording continues while this is non-empty, so an outer anchor still open
    /// keeps collecting after an inner one closes. The depth is what says when a
    /// scope closes: a scalar anchor closes at once, a collection anchor at its
    /// matching end event.
    open: Vec<(String, usize, usize)>,
    /// Nesting depth of the events read from the parser.
    ///
    /// Anchor spans are expressed in recorded events, so scope closing counts
    /// only what the parser produced -- not what a replay re-emits.
    record_depth: usize,
    /// Nesting depth of the events actually handed to the consumer.
    ///
    /// Distinct from `record_depth` because an alias splices a recorded value
    /// into a position that already has depth of its own. The parser bounds what
    /// it READS; this bounds what a consumer walks, which is the one that
    /// recurses through serde and consumes stack.
    depth: usize,
    /// Distinct anchor names declared so far, open ones included.
    ///
    /// Counted rather than read off `anchors.len()`: nested anchors are all open
    /// at once and none has been inserted yet, so a count taken at close time
    /// would let a document declare any number of them first.
    distinct_anchors: usize,
    /// The effective scalar cap: the smaller of the limit and any policy.
    max_scalar_length: usize,
    /// Spans still being replayed, innermost last.
    ///
    /// A stack rather than one cursor: an alias inside an anchored value expands
    /// while that value is itself being replayed.
    replaying: Vec<Range<usize>>,
    /// How many aliases have been expanded, and how many events that produced.
    ///
    /// Expansion is where a small document becomes a large one -- `&a`, then `*a`
    /// twice under `&b`, then `*b` twice under `&c` doubles at every level. The
    /// streaming path never materialises the result, so memory stays flat, but
    /// the time still grows exponentially. Both counters are charged before any
    /// replay begins.
    expansions: usize,
    expanded_events: usize,
    /// Limits governing the two counters above.
    limits: ResourceLimits,
}

impl<'a> Tape<'a> {
    /// Creates a tape over `input` with default parser configuration.
    // Test-only since #57: production reaches the tape through `with_config`,
    // and resolves aliases inside `next` rather than replaying spans by hand.
    // `cfg(test)` rather than an allow, so the compiler keeps enforcing that
    // nothing in production quietly starts depending on it.
    #[cfg(test)]
    pub(crate) fn new(input: &'a str) -> Self {
        Self::with_config(input, ParserConfig::default())
    }

    /// Creates a tape over `input` with caller-supplied parser configuration.
    pub(crate) fn with_config(input: &'a str, config: ParserConfig) -> Self {
        let limits = config.limits.clone();
        // The EFFECTIVE bound is the smaller of the two, which is what keeps the
        // policy a hardening knob: setting it above the limit cannot raise the
        // ceiling. Mirrors `Composer::compose_scalar`.
        let max_scalar_length = config
            .policies
            .max_scalar_length
            .unwrap_or(usize::MAX)
            .min(limits.max_scalar_length);
        Self {
            limits,
            max_scalar_length,
            parser: Parser::with_config(input, config),
            buffer: Vec::new(),
            anchors: HashMap::new(),
            open: Vec::new(),
            record_depth: 0,
            depth: 0,
            distinct_anchors: 0,
            replaying: Vec::new(),
            expansions: 0,
            expanded_events: 0,
        }
    }

    /// Yields the next event, resolving aliases and recording anchored spans.
    ///
    /// Recording is **lazy**: the buffer stays empty until an anchor is actually
    /// seen. Most documents contain none, and allocating a tape for them would
    /// make every ordinary parse pay for a feature it does not use.
    ///
    /// An alias never reaches the caller. It is replaced by the events of the
    /// value it names, so a consumer deserialises an aliased value with no
    /// knowledge that aliases exist. Doing this here rather than in the
    /// deserialiser is what keeps anchor bookkeeping in one place: the tape knows
    /// where every span starts and ends, and nothing else has to agree with it.
    pub(crate) fn next(&mut self) -> Option<Result<Event<'a>>> {
        self.next_inner().transpose()
    }

    /// The body of [`next`](Self::next), written so a check can just `?`.
    fn next_inner(&mut self) -> Result<Option<Event<'a>>> {
        loop {
            // `pull` reports where the event came from. Deciding that here
            // instead -- from `replaying.is_empty()` before the call -- reads an
            // exhausted frame that `pull` is about to discard, so the next PARSER
            // event is mistaken for a replayed one: it goes unrecorded and its
            // enclosing anchor scope never closes, surfacing later as a bogus
            // undefined alias. One source of truth avoids the ordering entirely.
            let Some((live, result)) = self.pull() else {
                return Ok(None);
            };
            let event = result?;

            if live {
                // Replayed events were checked when the parser produced them,
                // and re-checking would charge one document twice for the same
                // scalar.
                self.check_scalar_length(&event)?;
                self.record(&event)?;
            }

            // The ALIAS event is what gets recorded above, not its expansion.
            // Replaying a span that contains one therefore expands it again --
            // which is what makes `&z [*x]` reproduce `x` when `*z` is resolved.
            if let EventKind::Alias { name } = &event.kind {
                self.expand(name, event.span)?;
                continue;
            }

            if live {
                self.advance_scopes(&event);
            }

            // Every event counts here, replayed ones included: it is the emitted
            // stream a consumer walks.
            self.track_depth(&event)?;
            return Ok(Some(event));
        }
    }

    /// Refuses a scalar longer than the effective cap.
    fn check_scalar_length(&self, event: &Event<'a>) -> Result<()> {
        if let EventKind::Scalar { value, .. } = &event.kind
            && value.len() > self.max_scalar_length
        {
            return Err(Error::new(
                ErrorKind::ScalarLengthLimitExceeded {
                    limit: self.max_scalar_length,
                    actual: value.len(),
                },
                event.span,
            ));
        }
        Ok(())
    }

    /// Tracks the depth of the emitted stream and bounds it.
    const fn track_depth(&mut self, event: &Event<'a>) -> Result<()> {
        match event.kind {
            EventKind::SequenceStart { .. } | EventKind::MappingStart { .. } => {
                self.depth += 1;
                // The EFFECTIVE limit, as the parser uses: a `max_depth` above
                // `MAX_SAFE_DEPTH` would stop bounding memory and start deciding
                // whether the process survives.
                if self.depth > self.limits.effective_max_depth() {
                    return Err(Error::depth_exceeded(&self.limits, event.span));
                }
            }
            EventKind::SequenceEnd | EventKind::MappingEnd => {
                self.depth = self.depth.saturating_sub(1);
            }
            _ => {}
        }
        Ok(())
    }

    /// Takes the next event, and reports whether it came from the parser.
    ///
    /// Retiring finished replays and answering "is this live?" belong together:
    /// the answer is only correct once the exhausted frames are gone, and any
    /// caller that asked separately would have to get the order right.
    fn pull(&mut self) -> Option<(bool, Result<Event<'a>>)> {
        while let Some(range) = self.replaying.last_mut() {
            if range.start < range.end {
                let index = range.start;
                range.start += 1;
                // In bounds by construction, so indexing cannot panic on a
                // hostile document: `buffer` is append-only and every range is
                // built as `start..buffer.len()`, so `end` can never outrun it.
                return Some((false, Ok(self.buffer[index].clone())));
            }
            self.replaying.pop();
        }
        Some((true, self.parser.next_event()?))
    }

    /// Buffers an event when it may later need replaying, and opens any scope it
    /// declares.
    fn record(&mut self, event: &Event<'a>) -> Result<()> {
        let anchor = declared_anchor(event);

        if let Some(name) = anchor {
            // Both checks run BEFORE `to_owned`, because that call IS the
            // allocation. Checking afterwards would mean the process had already
            // paid the memory the cap exists to refuse.
            if name.len() > self.limits.max_anchor_name_length {
                return Err(Error::anchor_name_length_exceeded(&self.limits, event.span));
            }

            // Charged only for a name not already known. YAML permits an anchor
            // to be redefined, and a redefinition reuses its slot rather than
            // growing the map, so counting it would reject a legal document that
            // costs nothing extra. Open scopes count as known: a name is
            // declared before its value ends.
            let known = self.anchors.contains_key(name)
                || self.open.iter().any(|(open, _, _)| open == name);
            if !known {
                if self.distinct_anchors >= self.limits.max_anchors {
                    return Err(Error::anchor_count_exceeded(&self.limits, event.span));
                }
                self.distinct_anchors += 1;
            }
        }

        // Recorded when either a scope is open -- the event is part of some
        // anchored value -- or the event declares an anchor, since the span has
        // to start AT that event rather than after it.
        if !self.open.is_empty() || anchor.is_some() {
            self.buffer.push(event.clone());
        }

        if let Some(name) = anchor {
            let start = self.buffer.len().saturating_sub(1);
            self.open.push((name.to_owned(), start, self.record_depth));
        }
        Ok(())
    }

    /// Tracks nesting depth and closes every scope the event completes.
    fn advance_scopes(&mut self, event: &Event<'a>) {
        match event.kind {
            EventKind::SequenceStart { .. } | EventKind::MappingStart { .. } => {
                self.record_depth += 1;
            }
            EventKind::SequenceEnd | EventKind::MappingEnd => {
                // `saturating_sub`: an unbalanced end event is a malformed
                // document, which arrives from outside. It must not underflow.
                self.record_depth = self.record_depth.saturating_sub(1);
            }
            _ => {}
        }

        // A scope closes once the stream is back at the depth it opened at: a
        // scalar anchor never changed the depth, so it closes immediately, while
        // a collection anchor waits for its matching end event.
        while self
            .open
            .last()
            .is_some_and(|&(_, _, d)| d >= self.record_depth)
        {
            if let Some((name, start, _)) = self.open.pop() {
                // A repeated anchor name overwrites the earlier span, matching
                // YAML: an anchor may be redefined and a later alias refers to
                // the most recent definition.
                self.anchors.insert(name, start..self.buffer.len());
            }
        }
    }

    /// Queues the events of `name` for replay.
    ///
    /// An anchor still being read is deliberately absent from `anchors`, so
    /// `&x [*x]` reports an undefined alias instead of looping forever.
    fn expand(&mut self, name: &str, span: Span) -> Result<()> {
        let Some(range) = self.anchors.get(name).cloned() else {
            return Err(Error::new(
                ErrorKind::UndefinedAlias(name.to_string()),
                span,
            ));
        };

        // Charged BEFORE the replay is queued, so a document cannot spend the
        // budget and then be told it was over it.
        self.expansions += 1;
        if self.expansions > self.limits.max_alias_expansions {
            return Err(Error::new(
                ErrorKind::AliasExpansionLimitExceeded {
                    limit: self.limits.max_alias_expansions,
                },
                span,
            ));
        }

        self.expanded_events += range.len();
        if self.expanded_events > self.limits.max_total_alias_nodes {
            return Err(Error::new(
                ErrorKind::AliasMaterializationLimitExceeded {
                    limit: self.limits.max_total_alias_nodes,
                },
                span,
            ));
        }

        self.replaying.push(range);
        Ok(())
    }

    /// The events that built `name`, or `None` if it was never anchored.
    ///
    /// `None` rather than a panic: an undefined alias is a defect in the
    /// *document*, which arrives from outside, so it must be an error the caller
    /// can report rather than a crash.
    #[cfg(test)]
    pub(crate) fn replay(&self, name: &str) -> Option<&[Event<'a>]> {
        let range = self.anchors.get(name)?;
        self.buffer.get(range.clone())
    }

    /// How many events have been recorded.
    ///
    /// Exists for the test that pins the anchor-free path at zero. That is a
    /// performance requirement of #49 — "no allocation on the anchor-free path" —
    /// and an untested performance requirement is an aspiration.
    #[cfg(test)]
    pub(crate) const fn buffered_events(&self) -> usize {
        self.buffer.len()
    }
}

#[cfg(test)]
mod tests {
    use glaucus_core::error::ParserConfig;
    use glaucus_core::limits::ResourceLimits;

    use super::Tape;

    /// Drives the tape to exhaustion.
    ///
    /// Nothing to co-operate with: the tape opens and closes anchor scopes
    /// itself, so a consumer only has to keep calling `next`.
    fn drive(input: &str) -> Tape<'_> {
        let mut tape = Tape::new(input);
        while let Some(Ok(_)) = tape.next() {}
        tape
    }

    /// The performance requirement of #49, as a test rather than an aspiration:
    /// a document with no anchors must never allocate the buffer.
    #[test]
    fn an_anchor_free_document_records_nothing() {
        for input in [
            "a: 1\n",
            "a:\n  b:\n    - 1\n    - 2\n",
            "[1, 2, {a: b}]\n",
            "",
            "# comment only\n",
            "a: !!str 123\n",
        ] {
            let tape = drive(input);
            assert_eq!(
                tape.buffered_events(),
                0,
                "{input:?} has no anchors and must not be recorded"
            );
        }
    }

    #[test]
    fn a_scalar_anchor_records_and_replays() {
        let tape = drive("a: &x 1\nb: *x\n");
        let events = tape.replay("x").expect("x was anchored");
        assert_eq!(events.len(), 1, "a scalar anchor is one event");
        assert!(tape.buffered_events() > 0);
    }

    #[test]
    fn a_collection_anchor_replays_its_whole_span() {
        let tape = drive("a: &x [1, 2, 3]\nb: *x\n");
        let events = tape.replay("x").expect("x was anchored");
        // SequenceStart, three scalars, SequenceEnd.
        assert_eq!(events.len(), 5, "got {events:?}");
    }

    /// The reason the buffer is flat: `y` sits inside `x`, stored once.
    #[test]
    fn nested_anchors_share_one_buffer() {
        let tape = drive("a: &x {p: &y 1}\n");
        let x = tape.replay("x").expect("x").len();
        let y = tape.replay("y").expect("y").len();

        assert_eq!(y, 1, "y is the inner scalar");
        assert!(x > y, "x must contain y, got x={x} y={y}");
        assert_eq!(
            tape.buffered_events(),
            x,
            "the inner events must be stored once, not duplicated per anchor"
        );
    }

    /// An outer anchor still open must keep collecting after an inner one closes.
    #[test]
    fn recording_continues_until_the_last_scope_closes() {
        let tape = drive("a: &outer\n  inner: &in [1, 2]\n  after: 3\n");
        let outer = tape.replay("outer").expect("outer");
        let inner = tape.replay("in").expect("in");

        assert!(
            outer.len() > inner.len(),
            "outer must span past the inner anchor: outer={} inner={}",
            outer.len(),
            inner.len()
        );
    }

    #[test]
    fn an_undefined_alias_yields_none_not_a_panic() {
        let tape = drive("a: 1\n");
        assert!(tape.replay("nope").is_none());

        let tape = drive("a: &x 1\n");
        assert!(tape.replay("x").is_some());
        assert!(tape.replay("y").is_none());
    }

    /// YAML permits an anchor name to be redefined; a later alias refers to the
    /// most recent definition.
    #[test]
    fn a_redefined_anchor_replaces_the_earlier_span() {
        let tape = drive("a: &x 1\nb: &x [1, 2]\nc: *x\n");
        let events = tape.replay("x").expect("x");
        assert_eq!(events.len(), 4, "the sequence, not the earlier scalar");
    }

    #[test]
    fn an_unbalanced_end_event_does_not_underflow_the_depth() {
        // A stray `]` is a malformed document, which arrives from outside. The
        // depth counter must saturate rather than wrap to usize::MAX and leave
        // every later anchor scope permanently open.
        for input in ["a: 1\n]\n", "]\n", "a: &x [1]\n]\n"] {
            let tape = drive(input);
            let _ = tape.buffered_events();
        }
    }

    /// Drives the tape to exhaustion, returning the first error it reports.
    fn drive_err(input: &str, config: ParserConfig) -> Option<String> {
        let mut tape = Tape::with_config(input, config);
        while let Some(result) = tape.next() {
            if let Err(e) = result {
                return Some(e.to_string());
            }
        }
        None
    }

    /// Expansion is where a small document becomes a large one. The streaming
    /// path never materialises the result, so memory stays flat -- but the time
    /// still grows exponentially, so the budget has to be charged here too.
    #[test]
    fn the_alias_expansion_budget_is_enforced() {
        let mut config = ParserConfig::default();
        config.limits.max_alias_expansions = 1;

        let input = "a: &x 1\nb: *x\nc: *x\n";
        assert!(
            drive_err(input, config.clone()).is_some_and(|e| e.contains("alias expansion")),
            "a second expansion must exceed a budget of one"
        );

        // The budget is a ceiling, not a trap: one expansion is still allowed.
        assert_eq!(drive_err("a: &x 1\nb: *x\n", config), None);
    }

    /// A separate budget from the expansion count: few aliases can still name
    /// large spans, which is the amplification the count alone would not catch.
    #[test]
    fn the_total_expanded_event_budget_is_enforced() {
        let mut config = ParserConfig::default();
        config.limits.max_total_alias_nodes = 2;

        // `x` is five events (start, three scalars, end), so one alias to it
        // already exceeds a budget of two.
        let input = "a: &x [1, 2, 3]\nb: *x\n";
        assert!(
            drive_err(input, config.clone()).is_some_and(|e| e.contains("alias")),
            "a span larger than the budget must be refused"
        );

        // A one-event anchor stays under it.
        assert_eq!(drive_err("a: &x 1\nb: *x\n", config), None);
    }

    /// The document from #24, the P0 that put a budget on alias expansion.
    ///
    /// Refused by the expansion count rather than by the node total: the nine
    /// aliases at each level re-expand the level below, so the number of
    /// EXPANSIONS grows exponentially and trips first. Pinned here because the
    /// streaming path becoming the default must not quietly drop it.
    #[test]
    fn a_billion_laughs_document_is_refused() {
        let bomb = concat!(
            "a: &a [x,x,x,x,x,x,x,x,x]\n",
            "b: &b [*a,*a,*a,*a,*a,*a,*a,*a,*a]\n",
            "c: &c [*b,*b,*b,*b,*b,*b,*b,*b,*b]\n",
            "d: &d [*c,*c,*c,*c,*c,*c,*c,*c,*c]\n",
            "e: [*d,*d,*d,*d,*d,*d,*d,*d,*d]\n",
        );
        assert!(
            drive_err(bomb, ParserConfig::default()).is_some_and(|e| e.contains("alias")),
            "the streaming path must refuse a billion-laughs document"
        );
    }

    /// Expansion adds nesting the parser never counted.
    ///
    /// The parser bounds the depth of what it READS. An alias splices a recorded
    /// value into a position that already has depth of its own, so the stream a
    /// consumer actually walks can be deeper than anything the parser saw -- and
    /// it is that stream which recurses through serde and consumes stack.
    #[test]
    fn alias_expansion_cannot_push_nesting_past_max_depth() {
        let config = ParserConfig {
            limits: ResourceLimits {
                max_depth: 3,
                ..Default::default()
            },
            ..Default::default()
        };

        // Read on its own, nothing here exceeds depth 3; expanding `*a` inside
        // two more collections reaches 5.
        assert!(
            drive_err("a: &a [[1]]\nb: [[*a]]\n", config.clone())
                .is_some_and(|e| e.contains("depth")),
            "expansion must be bounded by the same depth limit"
        );

        // The same document without the alias stays within the limit.
        assert_eq!(drive_err("a: [[1]]\n", config), None);
    }

    #[test]
    fn max_anchors_is_enforced() {
        let config = ParserConfig {
            limits: ResourceLimits {
                max_anchors: 1,
                ..Default::default()
            },
            ..Default::default()
        };

        assert!(
            drive_err("a: &x 1\nb: &y 2\n", config.clone()).is_some_and(|e| e.contains("anchor")),
            "a second distinct anchor must be refused"
        );

        // A REDEFINITION reuses its slot, so it costs nothing and must be
        // allowed -- the tree path counts distinct names, not occurrences.
        assert_eq!(drive_err("a: &x 1\nb: &x 2\n", config.clone()), None);

        // Nested anchors are both open at once; neither has closed when the
        // second is declared, so a count taken only at close time would miss it.
        assert!(
            drive_err("a: &x [&y 1]\n", config).is_some_and(|e| e.contains("anchor")),
            "nested anchors must be counted while still open"
        );
    }

    #[test]
    fn max_anchor_name_length_is_enforced() {
        let config = ParserConfig {
            limits: ResourceLimits {
                max_anchor_name_length: 2,
                ..Default::default()
            },
            ..Default::default()
        };

        assert!(
            drive_err("a: &toolong 1\n", config.clone()).is_some_and(|e| e.contains("anchor")),
            "an over-long anchor name must be refused"
        );
        assert_eq!(drive_err("a: &ok 1\n", config), None);
    }

    #[test]
    fn max_scalar_length_is_enforced() {
        let config = ParserConfig {
            limits: ResourceLimits {
                max_scalar_length: 3,
                ..Default::default()
            },
            ..Default::default()
        };

        assert!(
            drive_err("a: abcdefghij\n", config.clone()).is_some_and(|e| e.contains("scalar")),
            "an over-long scalar must be refused"
        );
        assert_eq!(drive_err("a: ab\n", config), None);
    }
}

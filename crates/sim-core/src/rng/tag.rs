//! The type of the purpose tags. It has its own module so that its field is private even
//! to the rest of the random module: a pinned tag exists only through
//! `from_pinned_list`, and the gate counts the uses of that name in `tags.rs` against
//! `tests/golden/rng_tags.txt`.

/// What a draw is for. The values live in `tags`, are never renumbered and never 0.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct PurposeTag(u64);

impl PurposeTag {
    /// A tag of the pinned list: used once per tag in `tags.rs` and nowhere else.
    pub(super) const fn from_pinned_list(value: u64) -> PurposeTag {
        PurposeTag(value)
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

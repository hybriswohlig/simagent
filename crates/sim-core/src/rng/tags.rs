//! Purpose tags (ADR-0001 section 5): one per place that draws. Values run from 1, stay
//! below 2^32, are never renumbered; a deleted tag moves to `RETIRED`.
//! `tests/golden/rng_tags.txt` pins names and values.

use super::PurposeTag;

/// Start strand of a meiosis. subject: genome key of the child; occasion: parent side
/// (0 mother, 1 father); detail: section of the genome map.
pub const MEIOSIS_START_STRAND: PurposeTag = PurposeTag(1);

/// Which sex chromosome the father passes on. subject: genome key of the child;
/// occasion: 1 (father); detail: 0.
pub const SEX_CHROMOSOME: PurposeTag = PurposeTag(2);

/// Whether a couple conceives this month. subject: lineage key of the mother;
/// occasion: absolute month index; detail: lineage key of the father.
pub const CONCEPTION_ATTEMPT: PurposeTag = PurposeTag(3);

/// The budget of a cause of death. subject: lineage key; occasion: `str_key` of the
/// cause; detail: 0.
pub const DEATH_BUDGET: PurposeTag = PurposeTag(4);

/// Rank of a candidate in a pairing round. subject: lineage key of the seeker;
/// occasion: absolute index of the 28-day round; detail: the candidate.
pub const PAIRING_RANK: PurposeTag = PurposeTag(5);

/// Every tag in use with its name.
pub const ALL: &[(PurposeTag, &str)] = &[
    (MEIOSIS_START_STRAND, "MEIOSIS_START_STRAND"),
    (SEX_CHROMOSOME, "SEX_CHROMOSOME"),
    (CONCEPTION_ATTEMPT, "CONCEPTION_ATTEMPT"),
    (DEATH_BUDGET, "DEATH_BUDGET"),
    (PAIRING_RANK, "PAIRING_RANK"),
];

/// Values of deleted tags; they are never used again.
pub const RETIRED: &[u64] = &[];

#[cfg(test)]
mod tests {
    use super::*;

    /// ADR-0001 `tags_unique_and_pinned`.
    #[test]
    fn tags_unique_and_pinned() {
        let mut values: std::vec::Vec<u64> = ALL.iter().map(|(tag, _)| tag.get()).collect();
        for value in &values {
            assert!(*value != 0 && *value < (1 << 32), "{value}");
            assert!(!RETIRED.contains(value), "{value} is retired");
        }
        values.sort_unstable();
        values.dedup();
        assert_eq!(values.len(), ALL.len(), "tag values are unique");
        let mut names: std::vec::Vec<&str> = ALL.iter().map(|(_, name)| *name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), ALL.len(), "tag names are unique");

        let actual: std::string::String = ALL
            .iter()
            .map(|(tag, name)| std::format!("tag {} {name}\n", tag.get()))
            .chain(
                RETIRED
                    .iter()
                    .map(|value| std::format!("retired {value}\n")),
            )
            .collect();
        let golden = include_str!("../../../../tests/golden/rng_tags.txt");
        let pinned: std::string::String = golden
            .lines()
            .filter(|line| !line.starts_with('#'))
            .map(|line| std::format!("{line}\n"))
            .collect();
        assert_eq!(
            actual, pinned,
            "purpose tags are never renumbered (ADR-0001)"
        );
    }
}

//! Purpose tags (ADR-0001 section 5): one per place that draws. Values run from 1, stay
//! below 2^32, are never renumbered; a deleted tag moves to `RETIRED`.
//! `tests/golden/rng_tags.txt` pins names and values.

use super::PurposeTag;

/// Start strand of a segment in a meiosis. subject: genome key of the child; occasion:
/// parent side (0 mother, 1 father); detail: the segment (autosomes 0 to 7, the mother's
/// X 8).
pub const MEIOSIS_START_STRAND: PurposeTag = PurposeTag::from_pinned_list(1);

/// Which sex chromosome the father passes on. subject: genome key of the child;
/// occasion: 1 (father); detail: 0.
pub const SEX_CHROMOSOME: PurposeTag = PurposeTag::from_pinned_list(2);

/// Whether a couple conceives this month. subject: lineage key of the mother;
/// occasion: absolute month index; detail: lineage key of the father.
pub const CONCEPTION_ATTEMPT: PurposeTag = PurposeTag::from_pinned_list(3);

/// The budget of a cause of death. subject: lineage key; occasion: `str_key` of the
/// cause; detail: 0.
pub const DEATH_BUDGET: PurposeTag = PurposeTag::from_pinned_list(4);

/// Rank of a candidate in a pairing round. subject: lineage key of the seeker;
/// occasion: absolute index of the 28-day round; detail: the candidate.
pub const PAIRING_RANK: PurposeTag = PurposeTag::from_pinned_list(5);

/// Number of crossovers of a segment in a meiosis (genetics specification 2.3).
/// subject: genome key of the child; occasion: parent side (0 mother, 1 father); detail:
/// the segment (autosomes 0 to 7, the mother's X 8).
pub const CROSSOVER_COUNT: PurposeTag = PurposeTag::from_pinned_list(6);

/// Position of crossover `j` of a segment in a meiosis. subject: genome key of the
/// child; occasion: parent side; detail: `(segment << 32) | j`.
pub const CROSSOVER_POSITION: PurposeTag = PurposeTag::from_pinned_list(7);

/// Number of new mutations of a side in a meiosis (genetics specification 2.4).
/// subject: genome key of the child; occasion: parent side; detail: the part of the count
/// (0 while the total rate is at most 30).
pub const MUTATION_COUNT: PurposeTag = PurposeTag::from_pinned_list(8);

/// Site of new mutation `j` of a side in a meiosis. subject: genome key of the child;
/// occasion: parent side; detail: `j`.
pub const MUTATION_SITE: PurposeTag = PurposeTag::from_pinned_list(9);

/// Shared childhood environment `C_t` of a trait (genetics specification 4.2, ADR-0009).
/// subject: lineage key of the mother; occasion: `str_key` of the trait; detail: lineage
/// key of the father. Without parents in the run, the person's own key for both.
pub const ENV_SHARED: PurposeTag = PurposeTag::from_pinned_list(10);

/// Individual environment `E_t` of a trait (genetics specification 4.2, ADR-0009).
/// subject: the person's lineage key; occasion: `str_key` of the trait; detail: 0.
pub const ENV_INDIVIDUAL: PurposeTag = PurposeTag::from_pinned_list(11);

/// The population sample of the clamp rate of the body contract (ADR-0005 section 8,
/// ADR-0015 section 5). subject: `SubjectKey::WORLD`; occasion: the number of the sample;
/// detail: the part of it (0 the alleles, 1 to 19 the shared and individual environments,
/// 20 the age).
pub const BODY_CLAMP_SAMPLE: PurposeTag = PurposeTag::from_pinned_list(12);

/// Whether a person with the genotype of a hereditary disease has it (genetics
/// specification 3.1, ADR-0017 section 3). subject: the person's lineage key; occasion:
/// `str_key` of the disease; detail: 0.
pub const DISEASE_PENETRANCE: PurposeTag = PurposeTag::from_pinned_list(13);

/// The onset of a hereditary disease a person has (genetics specification 3.1). subject:
/// the person's lineage key; occasion: `str_key` of the disease; detail: 0.
pub const DISEASE_ONSET: PurposeTag = PurposeTag::from_pinned_list(14);

/// The offset of a woman's menarche after her puberty timing (genetics specification
/// 5.6, 11.7). subject: her lineage key; occasion: 0; detail: 0.
pub const FERTILITY_MENARCHE: PurposeTag = PurposeTag::from_pinned_list(15);

/// The draw of a woman's wish for children (genetics specification 11.3, 11.7). subject:
/// her lineage key; occasion: 0; detail: 0.
pub const FERTILITY_WISH: PurposeTag = PurposeTag::from_pinned_list(16);

/// Whether a conception gives one child, identical or fraternal twins (genetics
/// specification 11.3, 11.7). subject: the lineage key of the first child; occasion: 0;
/// detail: 0.
pub const MULTIPLE_BIRTH: PurposeTag = PurposeTag::from_pinned_list(17);

/// Every tag in use with its name.
pub const ALL: &[(PurposeTag, &str)] = &[
    (MEIOSIS_START_STRAND, "MEIOSIS_START_STRAND"),
    (SEX_CHROMOSOME, "SEX_CHROMOSOME"),
    (CONCEPTION_ATTEMPT, "CONCEPTION_ATTEMPT"),
    (DEATH_BUDGET, "DEATH_BUDGET"),
    (PAIRING_RANK, "PAIRING_RANK"),
    (CROSSOVER_COUNT, "CROSSOVER_COUNT"),
    (CROSSOVER_POSITION, "CROSSOVER_POSITION"),
    (MUTATION_COUNT, "MUTATION_COUNT"),
    (MUTATION_SITE, "MUTATION_SITE"),
    (ENV_SHARED, "ENV_SHARED"),
    (ENV_INDIVIDUAL, "ENV_INDIVIDUAL"),
    (BODY_CLAMP_SAMPLE, "BODY_CLAMP_SAMPLE"),
    (DISEASE_PENETRANCE, "DISEASE_PENETRANCE"),
    (DISEASE_ONSET, "DISEASE_ONSET"),
    (FERTILITY_MENARCHE, "FERTILITY_MENARCHE"),
    (FERTILITY_WISH, "FERTILITY_WISH"),
    (MULTIPLE_BIRTH, "MULTIPLE_BIRTH"),
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

        // `ALL` and `RETIRED` equal the golden, line by line. That no tag exists beside
        // `ALL` is the job of the gate (it counts the tags this file builds).
        let golden = include_str!("../../../../tests/golden/rng_tags.txt");
        let mut lines = golden.lines().filter(|line| !line.starts_with('#'));
        for (tag, name) in ALL {
            let line = lines.next().expect("a golden line per tag");
            let mut fields = line.split(' ');
            assert_eq!(fields.next(), Some("tag"), "{line}");
            let value = fields.next().and_then(|value| value.parse::<u64>().ok());
            assert_eq!(value, Some(tag.get()), "{line}");
            assert_eq!(fields.next(), Some(*name), "{line}");
            assert_eq!(fields.next(), None, "{line}");
        }
        for retired in RETIRED {
            let line = lines.next().expect("a golden line per retired value");
            let value = line
                .strip_prefix("retired ")
                .and_then(|value| value.parse::<u64>().ok());
            assert_eq!(value, Some(*retired), "{line}");
        }
        assert_eq!(
            lines.next(),
            None,
            "purpose tags are never renumbered (ADR-0001)"
        );
    }
}

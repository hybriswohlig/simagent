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

/// The budget of the separation of a partnership (ADR-0018 section 3). subject: the
/// woman's lineage key; occasion: the number of partnerships she formed before this one;
/// detail: the man's lineage key.
pub const SEPARATION_BUDGET: PurposeTag = PurposeTag::from_pinned_list(18);

/// Whether a single woman of the market age searches in a round (genetics specification
/// 11.2, 11.7). subject: her lineage key; occasion: the month index of the round; detail: 0.
pub const MARKET_SEARCH: PurposeTag = PurposeTag::from_pinned_list(19);

/// The order in which the searching women of a round choose: a rank (11.2, 11.7, ADR-0018
/// section 1). subject: the world; occasion: the month index of the round; detail: her
/// lineage key.
pub const MARKET_ORDER: PurposeTag = PurposeTag::from_pinned_list(20);

/// The Gumbel noise of the utility of a candidate (11.2, 11.7). subject: the searching
/// woman's lineage key; occasion: the month index of the round; detail: the candidate's
/// lineage key.
pub const PAIRING_NOISE: PurposeTag = PurposeTag::from_pinned_list(21);

/// The age at which a person leaves home, from the table of the data (ADR-0018 section
/// 4). subject: the person's lineage key; occasion: 0; detail: 0.
pub const LEAVE_HOME: PurposeTag = PurposeTag::from_pinned_list(22);

/// The foster mother of a group of orphans: the rank of `k_smallest` (ADR-0018 section 4).
/// subject: the lineage key of the group's eldest child; occasion: the month index of the
/// placement; detail: the candidate woman's lineage key.
pub const FOSTER_RANK: PurposeTag = PurposeTag::from_pinned_list(23);

/// Whether a household emigrates above the soft capacity: Bernoulli (ADR-0020 section 6).
/// subject: the smallest lineage key of its members of 18 or more; occasion: the calendar
/// year; detail: 0.
pub const EMIGRATION: PurposeTag = PurposeTag::from_pinned_list(24);

/// An allele of a founder of the pre-run, drawn by `cargo xtask prerun` (ADR-0020 section
/// 3): Hardy-Weinberg at frequency 0.5, alternative if `unit < 0.5`. subject: the founder's
/// lineage key; occasion: `str_key` of the locus; detail: the copy (0 maternal, 1 paternal).
pub const PRERUN_ALLELE: PurposeTag = PurposeTag::from_pinned_list(25);

/// An allele of a pool person at one of the seven loci the pre-run left at the reference
/// allele (ADR-0020 section 4, G1 and G2): subject the person's genome key, occasion
/// `str_key` of the locus, detail the gamete (0 maternal, 1 paternal); the pool seed.
pub const POOL_ALLELE: PurposeTag = PurposeTag::from_pinned_list(26);

/// The order of the households of the pool for a town's founders and each year's
/// immigrants (ADR-0020 section 5): a rank, subject `WORLD`, occasion 0 at `Start` and the
/// year for immigration, candidate the smallest lineage key of a member of 18 or more.
pub const POOL_RANK: PurposeTag = PurposeTag::from_pinned_list(27);

/// An allele of a founder of `Start` at a locus it does not name (Grok F1): alternative if
/// `unit < q`, `q = d_ref / 2` at an additive locus, the pool's frequency at the seven pool
/// loci. subject: the founder's lineage key; occasion: `str_key` of the locus; detail: the
/// copy as a gamete (0 maternal, 1 paternal; Y 1, mt 0); the world seed.
pub const FOUNDER_ALLELE: PurposeTag = PurposeTag::from_pinned_list(28);

/// A first name (ADR-0021 section 3, presentation only). subject: the person's lineage key;
/// occasion: its sex (0 female, 1 male); detail: 0; the world seed.
pub const FIRST_NAME: PurposeTag = PurposeTag::from_pinned_list(29);

/// A surname (ADR-0021 section 3, presentation only). subject: the lineage key of the root
/// of the paternal chain; occasion: 0; detail: 0; the world seed.
pub const SURNAME: PurposeTag = PurposeTag::from_pinned_list(30);

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
    (SEPARATION_BUDGET, "SEPARATION_BUDGET"),
    (MARKET_SEARCH, "MARKET_SEARCH"),
    (MARKET_ORDER, "MARKET_ORDER"),
    (PAIRING_NOISE, "PAIRING_NOISE"),
    (LEAVE_HOME, "LEAVE_HOME"),
    (FOSTER_RANK, "FOSTER_RANK"),
    (EMIGRATION, "EMIGRATION"),
    (PRERUN_ALLELE, "PRERUN_ALLELE"),
    (POOL_ALLELE, "POOL_ALLELE"),
    (POOL_RANK, "POOL_RANK"),
    (FOUNDER_ALLELE, "FOUNDER_ALLELE"),
    (FIRST_NAME, "FIRST_NAME"),
    (SURNAME, "SURNAME"),
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

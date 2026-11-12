//! The rules of a talk (ADR-0030 section 3): the house day, whether a pair may begin, which
//! topic they take, who acts and which bond steps the outcome writes. Interests are one draw
//! per topic. Klatsch is allowed when the pair's `gossip` flag is set (ADR-0031 section 3).

use alloc::vec;
use alloc::vec::Vec;

use crate::data_id::DataId;
use crate::ids::SimId;
use crate::lineage::LineageKey;
use crate::rng::tags::{TALK_INTEREST, TALK_LIE, TALK_REFUTE};
use crate::rng::{DrawKey, Draws, SubjectKey, WorldSeed};
use crate::social::bond::{AFFECTION, ATTRACTION, FAMILIARITY, GRUDGE, RESPECT, TRUST};
use crate::social::data::{BondValue, Condition, Limits, Occasion, SocialData, Who};
use crate::social::memory::MOST;
use crate::time::{DAY, SimTime};

/// The six topics in the order of `data/social/social.json` (ADR-0030 section 3).
pub const TOPICS: usize = 6;

/// One directed view of a pair at the second being judged. `a` is the smaller [`SimId`].
pub struct TalkPair<'a> {
    pub a: SimId,
    pub b: SimId,
    pub age_a: u32,
    pub age_b: u32,
    pub edge_ab: [u16; 6],
    pub edge_ba: [u16; 6],
    pub interest_a: &'a [u16],
    pub interest_b: &'a [u16],
    /// A common memory exists, so Erinnern may be allowed.
    pub common_memory: bool,
    /// One of the two has gossip material, or a reason to lie (ADR-0031 section 3).
    pub gossip: bool,
}

/// The counters a beginning reads. `planned` is the second being judged.
pub struct TalkFacts {
    pub open_talk: bool,
    pub last_talk: Option<SimTime>,
    pub talks_a: u16,
    pub talks_b: u16,
    pub entries_a: u16,
    pub entries_b: u16,
    pub age_a: u32,
    pub age_b: u32,
}

/// One edge an outcome changes, `owner`'s bond to `other`, in the bond's index order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Outcome {
    pub owner: SimId,
    pub other: SimId,
    pub steps: Vec<(usize, i16)>,
}

/// The house day of `t` counted from `move_in` (ADR-0030 section 3, "Haustag").
///
/// READING: the code's `HouseMorning` fires once (ADR-0024), while the day the counters use
/// is every 06:00 boundary, as the reference counts it: `(t - 06:00) / 86400` minus the same
/// for the move-in second. SimTime 0 is midnight. A move-in at 20:00 is day 0 until the next
/// 06:00.
pub fn house_day(t: SimTime, move_in: SimTime) -> i64 {
    let six = 6 * 3600;
    let day = DAY.as_secs();
    (t.as_secs() - six).div_euclid(day) - (move_in.as_secs() - six).div_euclid(day)
}

/// Whether the pair may talk at `planned`. Planning passes `check_open`; execution does not
/// (the open Talk is the one running). The gap is strict: exactly `pair_gap_seconds` passes
/// and one second less does not. Both need `talk_entries` free of `entries_per_day`.
pub fn talk_conditions(
    limits: &Limits,
    planned: SimTime,
    facts: &TalkFacts,
    check_open: bool,
) -> bool {
    if facts.age_a < u32::from(limits.min_age) || facts.age_b < u32::from(limits.min_age) {
        return false;
    }
    if check_open && facts.open_talk {
        return false;
    }
    if let Some(last) = facts.last_talk
        && planned.as_secs() - last.as_secs() < i64::from(limits.pair_gap_seconds)
    {
        return false;
    }
    if facts.talks_a >= limits.talks_per_day || facts.talks_b >= limits.talks_per_day {
        return false;
    }
    let free = |used: u16| limits.entries_per_day.saturating_sub(used);
    if free(facts.entries_a) < limits.talk_entries || free(facts.entries_b) < limits.talk_entries {
        return false;
    }
    true
}

/// The interests of one person, `uniform_below(1001)` of [`TALK_INTEREST`], subject the
/// lineage key, occasion the topic number, detail 0 (ADR-0030 section 3).
pub fn interests(seed: WorldSeed, lineage: LineageKey) -> [u16; TOPICS] {
    let draws = Draws::new(seed);
    core::array::from_fn(|topic| {
        let key = DrawKey {
            tag: TALK_INTEREST,
            subject: SubjectKey::from(lineage),
            occasion: u64::try_from(topic).unwrap_or_else(|_| panic!("invariant talk.interest")),
            detail: 0,
        };
        let drawn = draws.uniform_below(key, 1001);
        u16::try_from(drawn).unwrap_or_else(|_| panic!("invariant talk.interest"))
    })
}

/// The lie draw (ADR-0031 section 4). Subject the speaker's lineage key, occasion the
/// second of the Talk, detail the hearer's lineage key. The world seed.
pub fn lie_draw_key(speaker: LineageKey, second: SimTime, hearer: LineageKey) -> DrawKey {
    DrawKey {
        tag: TALK_LIE,
        subject: SubjectKey::from(speaker),
        occasion: u64::try_from(second.as_secs()).unwrap_or_else(|_| panic!("invariant talk.lie")),
        detail: hearer.get(),
    }
}

/// The refutation draw (ADR-0031 section 5). Subject the refuter's lineage key, occasion
/// the second, detail the hearer's lineage key. The world seed.
pub fn refute_draw_key(refuter: LineageKey, second: SimTime, hearer: LineageKey) -> DrawKey {
    DrawKey {
        tag: TALK_REFUTE,
        subject: SubjectKey::from(refuter),
        occasion: u64::try_from(second.as_secs())
            .unwrap_or_else(|_| panic!("invariant talk.refute")),
        detail: hearer.get(),
    }
}

/// `uniform_below(1_000_000)` of a lie or a refutation key.
pub fn talk_ppm(seed: WorldSeed, key: DrawKey) -> u64 {
    Draws::new(seed).uniform_below(key, 1_000_000)
}

/// Alltag, the topic of the data's first place (ADR-0030 section 3).
pub const EVERYDAY: usize = 0;
/// Erinnern.
pub const RECALL: usize = 4;
/// The topic numbers with an actor (ADR-0030 section 3: 1 Lob, 2 Necken, 5 Entschuldigung). The
/// number is the topic's place in the data and the occasion of its interest draw.
pub const PRAISE: usize = 1;
/// Necken.
pub const TEASE: usize = 2;
/// Klatsch.
pub const GOSSIP: usize = 3;
/// Entschuldigung.
pub const APOLOGY: usize = 5;

/// The feelings a topic's outcome brings (ADR-0032 section 3): who feels it, which row of the
/// table, and the person its band reads the affection toward. Alltag and Erinnern both, each toward
/// the other; Lob the praised toward the praiser, Necken the teased toward the teaser,
/// Entschuldigung the one grudged, toward the one who apologises (the addressed person of the
/// outcome, as `topic_outcome` has it). Klatsch has no row of its own: it acts through the heard
/// deed and the refutation.
pub fn topic_feelings(
    topic: usize,
    pair: &TalkPair<'_>,
    actor: Option<SimId>,
) -> Vec<(SimId, Occasion, Option<SimId>)> {
    let (a, b) = (pair.a, pair.b);
    let both = |occasion| vec![(a, occasion, Some(b)), (b, occasion, Some(a))];
    let addressed = |occasion| {
        let actor = actor.unwrap_or_else(|| panic!("invariant talk.actor"));
        let other = if actor == a { b } else { a };
        vec![(other, occasion, Some(actor))]
    };
    match topic {
        EVERYDAY => both(Occasion::TalkEveryday),
        PRAISE => addressed(Occasion::TalkPraise),
        TEASE => addressed(Occasion::TalkTease),
        RECALL => both(Occasion::TalkRecall),
        APOLOGY => addressed(Occasion::TalkApology),
        _ => Vec::new(),
    }
}

/// What a recall writes: the strength at now plus the recall step, at most 1000.
pub fn recall_strength(strength: u16, step: u16) -> u16 {
    let sum = u32::from(strength).saturating_add(u32::from(step));
    u16::try_from(sum.min(u32::from(MOST))).unwrap_or_else(|_| panic!("invariant talk.recall"))
}

/// Who praises, teases or apologises. Other topics have no actor. Lob and Necken: the larger
/// interest, a tie the smaller [`SimId`]. Entschuldigung: the smaller own-edge grudge, a tie
/// the smaller [`SimId`] (ADR-0030 section 3; the reference's reading of "kleinerer Groll").
pub fn topic_actor(_social: &SocialData, topic: usize, pair: &TalkPair<'_>) -> Option<SimId> {
    if topic == PRAISE || topic == TEASE {
        let ia = interest_at(pair.interest_a, topic);
        let ib = interest_at(pair.interest_b, topic);
        Some(if ib > ia { pair.b } else { pair.a })
    } else if topic == APOLOGY {
        let ga = pair.edge_ab[GRUDGE];
        let gb = pair.edge_ba[GRUDGE];
        Some(if gb < ga { pair.b } else { pair.a })
    } else {
        None
    }
}

/// Whether topic `topic` is allowed at `furniture` for this pair: both ages inside the
/// topic's bounds, the furniture one the topic names, and the condition. Alltag's condition
/// is always, on its own furniture.
///
/// READING: the reference's `topic_allowed` does not look at furniture or the data age
/// bounds. The ADR and the data do, so this does too. On a dining table, a sofa or a
/// television the two readings agree for the six topics, because Alltag names all three and
/// Necken's bound is the data `max_age`.
pub fn topic_allowed(
    social: &SocialData,
    topic: usize,
    pair: &TalkPair<'_>,
    furniture: &DataId,
) -> bool {
    let Some(row) = social.topics.get(topic) else {
        return false;
    };
    let age_ok = |age: u32| (u32::from(row.min_age)..=u32::from(row.max_age)).contains(&age);
    if !age_ok(pair.age_a) || !age_ok(pair.age_b) {
        return false;
    }
    if !row.furniture.iter().any(|kind| kind == furniture) {
        return false;
    }
    match row.condition {
        Condition::Always => true,
        Condition::AffectionFrom(least) => match topic_actor(social, topic, pair) {
            Some(actor) => edge_of(pair, actor)[AFFECTION] >= least,
            None => false,
        },
        Condition::GrudgeFrom(least) => pair.edge_ab[GRUDGE].max(pair.edge_ba[GRUDGE]) >= least,
        Condition::CommonMemory => pair.common_memory,
        Condition::Gossip => pair.gossip,
    }
}

/// The allowed topic with the largest interest sum, the smallest number on a tie.
///
/// # Panics
///
/// Invariant `talk.topic` if none is allowed (a legal sociable seat always has Alltag) or
/// `talk.pair` if `a` is not the smaller id. Invariant `talk.interests` if an interest row
/// is shorter than the topic list.
pub fn choose_topic(social: &SocialData, pair: &TalkPair<'_>, furniture: &DataId) -> usize {
    if pair.a >= pair.b {
        panic!("invariant talk.pair");
    }
    let mut best: Option<(i32, usize)> = None;
    for topic in 0..social.topics.len() {
        if !topic_allowed(social, topic, pair, furniture) {
            continue;
        }
        let sum = i32::from(interest_at(pair.interest_a, topic))
            + i32::from(interest_at(pair.interest_b, topic));
        let score = (-sum, topic);
        if best.is_none_or(|old| score < old) {
            best = Some(score);
        }
    }
    best.map(|(_, topic)| topic)
        .unwrap_or_else(|| panic!("invariant talk.topic"))
}

/// The outcome as edge steps, the smaller id's edge first. Steps of one edge are one change.
/// Klatsch has none. The recall of Erinnern is not a bond step.
pub fn topic_outcome(social: &SocialData, topic: usize, pair: &TalkPair<'_>) -> Vec<Outcome> {
    let Some(row) = social.topics.get(topic) else {
        return Vec::new();
    };
    let mut ab = Vec::new();
    let mut ba = Vec::new();
    for step in &row.steps {
        let index = value_index(step.value);
        match step.who {
            Who::Both => {
                ab.push((index, step.delta));
                ba.push((index, step.delta));
            }
            Who::Target => {
                let actor = topic_actor(social, topic, pair)
                    .unwrap_or_else(|| panic!("invariant talk.actor"));
                // The addressed person is the other one: praised, teased, or the one grudged.
                if actor == pair.a {
                    ba.push((index, step.delta));
                } else {
                    ab.push((index, step.delta));
                }
            }
        }
    }
    let mut out = Vec::new();
    if !ab.is_empty() {
        out.push(Outcome {
            owner: pair.a,
            other: pair.b,
            steps: ab,
        });
    }
    if !ba.is_empty() {
        out.push(Outcome {
            owner: pair.b,
            other: pair.a,
            steps: ba,
        });
    }
    out
}

fn interest_at(interests: &[u16], topic: usize) -> u16 {
    interests
        .get(topic)
        .copied()
        .unwrap_or_else(|| panic!("invariant talk.interests"))
}

fn edge_of(pair: &TalkPair<'_>, owner: SimId) -> [u16; 6] {
    if owner == pair.a {
        pair.edge_ab
    } else {
        pair.edge_ba
    }
}

fn value_index(value: BondValue) -> usize {
    match value {
        BondValue::Affection => AFFECTION,
        BondValue::Trust => TRUST,
        BondValue::Respect => RESPECT,
        BondValue::Attraction => ATTRACTION,
        BondValue::Familiarity => FAMILIARITY,
        BondValue::Grudge => GRUDGE,
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use super::*;

    fn social() -> SocialData {
        let raw: SocialData =
            serde_json::from_str(include_str!("../../../../data/social/social.json"))
                .expect("social json");
        raw.checked().expect("social checks")
    }

    fn sim(id: u64) -> SimId {
        SimId::try_from(id).expect("sim")
    }

    fn at(secs: i64) -> SimTime {
        SimTime::from_secs(secs)
    }

    fn facts(age_a: u32, age_b: u32) -> TalkFacts {
        TalkFacts {
            open_talk: false,
            last_talk: None,
            talks_a: 0,
            talks_b: 0,
            entries_a: 0,
            entries_b: 0,
            age_a,
            age_b,
        }
    }

    fn pair<'a>(
        ea: [u16; 6],
        eb: [u16; 6],
        ia: &'a [u16],
        ib: &'a [u16],
        age_a: u32,
        age_b: u32,
        memory: bool,
    ) -> TalkPair<'a> {
        TalkPair {
            a: sim(1),
            b: sim(2),
            age_a,
            age_b,
            edge_ab: ea,
            edge_ba: eb,
            interest_a: ia,
            interest_b: ib,
            common_memory: memory,
            gossip: false,
        }
    }

    #[test]
    fn planning_and_the_house_day_match_the_reference() {
        let limits = &social().limits;
        let planned = at(100_000 + i64::from(limits.talk_delay_seconds));
        let base = facts(30, 30);
        assert!(talk_conditions(limits, planned, &base, true));
        let mut open = facts(30, 30);
        open.open_talk = true;
        assert!(!talk_conditions(limits, planned, &open, true));
        assert!(talk_conditions(limits, planned, &open, false));
        let mut gap = facts(30, 30);
        gap.last_talk = Some(at(planned.as_secs() - 3599));
        assert!(!talk_conditions(limits, planned, &gap, true));
        gap.last_talk = Some(at(planned.as_secs() - 3600));
        assert!(talk_conditions(limits, planned, &gap, true));
        let mut talks = facts(30, 30);
        talks.talks_b = 7;
        assert!(talk_conditions(limits, planned, &talks, true));
        talks.talks_a = 8;
        talks.talks_b = 0;
        assert!(!talk_conditions(limits, planned, &talks, true));
        let mut entries = facts(30, 30);
        entries.entries_a = 18;
        assert!(talk_conditions(limits, planned, &entries, true));
        entries.entries_a = 0;
        entries.entries_b = 19;
        assert!(!talk_conditions(limits, planned, &entries, true));
        assert!(!talk_conditions(limits, planned, &facts(5, 30), true));
        assert!(talk_conditions(limits, planned, &facts(30, 6), true));
        let move_in = 20 * 3600;
        let day = DAY.as_secs();
        assert_eq!(house_day(at(6 * 3600 + day), at(move_in)), 1);
        assert_eq!(house_day(at(6 * 3600 + day - 1), at(move_in)), 0);
        assert_eq!(house_day(at(6 * 3600 + 2 * day), at(move_in)), 2);
    }

    #[test]
    fn topics_actors_and_outcomes_match_the_reference() {
        let social = social();
        let dining = DataId::new("core:dining_table").expect("id");
        let low = [400, 450, 400, 0, 200, 0];
        let zeros = [0; 6];
        let p = pair(low, low, &zeros, &zeros, 30, 32, false);
        let allowed: Vec<bool> = (0..6)
            .map(|t| topic_allowed(&social, t, &p, &dining))
            .collect();
        assert_eq!(allowed, vec![true, false, true, false, false, false]);
        assert_eq!(choose_topic(&social, &p, &dining), 0);
        let ia = [10, 0, 500, 0, 0, 0];
        let ib = [10, 0, 300, 0, 0, 0];
        assert_eq!(
            choose_topic(&social, &pair(low, low, &ia, &ib, 30, 32, false), &dining),
            2
        );
        let tie = [500, 0, 500, 0, 0, 0];
        assert_eq!(
            choose_topic(
                &social,
                &pair(low, low, &tie, &zeros, 30, 32, false),
                &dining
            ),
            0
        );
        assert!(!topic_allowed(
            &social,
            2,
            &pair(low, low, &zeros, &zeros, 30, 60, false),
            &dining
        ));
        assert!(topic_allowed(
            &social,
            2,
            &pair(low, low, &zeros, &zeros, 59, 59, false),
            &dining
        ));
        let like = [500, 450, 400, 0, 200, 0];
        let a_less = [0, 100, 0, 0, 0, 0];
        let b_more = [0, 200, 0, 0, 0, 0];
        let lob = pair(like, low, &a_less, &b_more, 30, 32, false);
        assert_eq!(topic_actor(&social, 1, &lob), Some(sim(2)));
        assert!(!topic_allowed(&social, 1, &lob, &dining));
        let tied = pair(like, low, &a_less, &a_less, 30, 32, false);
        assert_eq!(topic_actor(&social, 1, &tied), Some(sim(1)));
        assert!(topic_allowed(&social, 1, &tied, &dining));
        let praised = topic_outcome(&social, 1, &tied);
        assert_eq!(praised.len(), 1);
        assert_eq!(praised[0].owner, sim(2));
        assert_eq!(praised[0].other, sim(1));
        assert_eq!(praised[0].steps, vec![(AFFECTION, 30), (RESPECT, 20)]);
        let neck = pair(
            low,
            low,
            &[0, 0, 100, 0, 0, 0],
            &[0, 0, 200, 0, 0, 0],
            30,
            32,
            false,
        );
        let teased = topic_outcome(&social, 2, &neck);
        assert_eq!(teased.len(), 1);
        assert_eq!((teased[0].owner, teased[0].other), (sim(1), sim(2)));
        assert_eq!(teased[0].steps, vec![(GRUDGE, 60)]);
        let everyday = topic_outcome(&social, 0, &p);
        assert_eq!(everyday.len(), 2);
        assert_eq!(everyday[0].steps, vec![(FAMILIARITY, 40)]);
        assert_eq!(everyday[1].steps, vec![(FAMILIARITY, 40)]);
        assert!(!topic_allowed(&social, 4, &p, &dining));
        let remembered = pair(low, low, &zeros, &zeros, 30, 32, true);
        assert!(topic_allowed(&social, 4, &remembered, &dining));
        let recall = topic_outcome(&social, 4, &p);
        assert_eq!(recall[0].steps, vec![(AFFECTION, 20)]);
        assert_eq!(recall[1].steps, vec![(AFFECTION, 20)]);
        assert!(!topic_allowed(&social, 3, &p, &dining));
        let mut with_material = p;
        with_material.gossip = true;
        assert!(topic_allowed(&social, 3, &with_material, &dining));
        let grudge = |g: u16| [400, 450, 400, 0, 200, g];
        assert!(!topic_allowed(
            &social,
            5,
            &pair(grudge(299), grudge(299), &zeros, &zeros, 30, 32, false),
            &dining
        ));
        let q = pair(grudge(300), grudge(0), &zeros, &zeros, 30, 32, false);
        assert!(topic_allowed(&social, 5, &q, &dining));
        assert_eq!(topic_actor(&social, 5, &q), Some(sim(2)));
        let sorry = topic_outcome(&social, 5, &q);
        assert_eq!(sorry.len(), 1);
        assert_eq!((sorry[0].owner, sorry[0].other), (sim(1), sim(2)));
        assert_eq!(sorry[0].steps, vec![(GRUDGE, -200), (TRUST, 60)]);
        let q2 = pair(grudge(500), grudge(400), &zeros, &zeros, 30, 32, false);
        assert_eq!(topic_actor(&social, 5, &q2), Some(sim(2)));
        let both = topic_outcome(&social, 5, &q2);
        assert_eq!(
            (both[0].owner, both[0].steps.clone()),
            (sim(1), vec![(GRUDGE, -200), (TRUST, 60)])
        );
        let q3 = pair(grudge(400), grudge(400), &zeros, &zeros, 30, 32, false);
        assert_eq!(topic_actor(&social, 5, &q3), Some(sim(1)));
        let other = topic_outcome(
            &social,
            5,
            &pair(grudge(0), grudge(350), &zeros, &zeros, 30, 32, false),
        );
        assert_eq!((other[0].owner, other[0].other), (sim(2), sim(1)));
        assert_eq!(other[0].steps, vec![(GRUDGE, -200), (TRUST, 60)]);
    }

    #[test]
    fn a_recall_adds_300_and_stops_at_1000() {
        let step = social().memory.recall_step;
        assert_eq!(step, 300);
        assert_eq!(recall_strength(400, step), 700);
        assert_eq!(recall_strength(800, step), 1000);
        assert_eq!(recall_strength(900, step), 1000);
    }

    #[test]
    fn interest_pins_of_the_social_founders_choose_the_reference_topics() {
        let tag = DataId::new("test:social").expect("tag");
        let keys = [
            LineageKey::founder(&tag, 0),
            LineageKey::founder(&tag, 1),
            LineageKey::founder(&tag, 2),
            LineageKey::founder(&tag, 3),
        ];
        assert_eq!(keys[0].get(), 0x61e1_535e_34ec_b28b);
        assert_eq!(keys[1].get(), 0xff0d_b729_371d_f98c);
        assert_eq!(keys[2].get(), 0x3b42_45d0_b56c_e9f8);
        assert_eq!(keys[3].get(), 0xd1f7_7e44_4ec7_0f31);
        let seed = WorldSeed::new(0x5EED);
        let rows = [
            interests(seed, keys[0]),
            interests(seed, keys[1]),
            interests(seed, keys[2]),
            interests(seed, keys[3]),
        ];
        assert_eq!(rows[0], [191, 614, 996, 932, 691, 609]);
        assert_eq!(rows[1], [818, 320, 543, 436, 410, 736]);
        assert_eq!(rows[2], [44, 850, 922, 380, 111, 824]);
        assert_eq!(rows[3], [947, 239, 296, 644, 8, 434]);
        // The reference table of TALK_LIE and TALK_REFUTE (tools/social_reference.py), seed
        // 0x5EED, founders of test:social. A lie is u < 50_000. A refutation is u < 500_000.
        let ppm = |speaker: usize, second: i64, hearer: usize, lie: bool| {
            let key = if lie {
                lie_draw_key(keys[speaker], at(second), keys[hearer])
            } else {
                refute_draw_key(keys[speaker], at(second), keys[hearer])
            };
            talk_ppm(seed, key)
        };
        let row = |second: i64, speaker: usize, hearer: usize, lie_u: u64, refute_u: u64| {
            assert_eq!(ppm(speaker, second, hearer, true), lie_u);
            assert_eq!(ppm(speaker, second, hearer, false), refute_u);
        };
        row(21_600, 0, 1, 242_294, 785_391);
        row(21_600, 1, 0, 219_011, 252_573);
        row(21_600, 2, 3, 617_183, 356_312);
        row(21_600, 3, 2, 502_153, 839_005);
        row(25_200, 0, 1, 875_054, 259_019);
        row(25_200, 1, 0, 956_384, 999_349);
        row(25_200, 2, 3, 831_587, 647_118);
        row(25_200, 3, 2, 63_792, 745_268);
        row(237_600, 0, 1, 925_772, 591_903);
        row(237_600, 1, 0, 275_257, 577_142);
        row(237_600, 2, 3, 114_266, 694_093);
        row(237_600, 3, 2, 483_051, 816_114);
        assert_eq!(ppm(0, 23_400, 1, true), 22_823);
        assert!(ppm(0, 23_400, 1, true) < 50_000);
        assert_eq!(ppm(0, 22_800, 1, true), 68_860);
        assert_eq!(ppm(0, 114_000, 1, true), 38_119);
        assert_eq!(ppm(0, 229_800, 1, true), 3_870);
        assert_eq!(ppm(0, 324_000, 1, true), 15_212);
        assert_eq!(ppm(2, 27_000, 1, false), 218_846);
        assert_eq!(ppm(2, 30_600, 1, false), 533_784);
        let social = social();
        let dining = DataId::new("core:dining_table").expect("id");
        let house = [400, 450, 400, 0, 200, 0];
        let partner = [700, 700, 500, 500, 600, 0];
        let chosen = |ia: &[u16], ib: &[u16], ea: [u16; 6], eb: [u16; 6], memory: bool| {
            let view = TalkPair {
                a: sim(1),
                b: sim(2),
                age_a: 30,
                age_b: 32,
                edge_ab: ea,
                edge_ba: eb,
                interest_a: ia,
                interest_b: ib,
                common_memory: memory,
                gossip: false,
            };
            let topic = choose_topic(&social, &view, &dining);
            (topic, topic_actor(&social, topic, &view))
        };
        assert_eq!(
            chosen(&rows[0], &rows[1], house, house, false),
            (2, Some(sim(1)))
        );
        assert_eq!(
            chosen(&rows[0], &rows[1], partner, partner, false),
            (2, Some(sim(1)))
        );
        assert_eq!(
            chosen(&rows[0], &rows[1], partner, partner, true),
            (2, Some(sim(1)))
        );
        let mut grudging = house;
        grudging[GRUDGE] = 400;
        assert_eq!(
            chosen(&rows[0], &rows[1], grudging, house, false),
            (2, Some(sim(1)))
        );
        assert_eq!(chosen(&rows[1], &rows[3], house, house, false), (0, None));
        assert_eq!(
            chosen(&rows[1], &rows[3], partner, partner, true),
            (0, None)
        );
        assert_eq!(
            chosen(&rows[2], &rows[3], house, house, false),
            (2, Some(sim(1)))
        );
        assert_eq!(
            chosen(&rows[2], &rows[3], grudging, house, false),
            (5, Some(sim(2)))
        );
        let mut quiet = pair(partner, partner, &rows[0], &rows[1], 30, 32, true);
        assert!(!topic_allowed(&social, 3, &quiet, &dining));
        quiet.gossip = true;
        assert!(topic_allowed(&social, 3, &quiet, &dining));
        // Necken's sum is 1539 and Klatsch's is 1368, so Necken still wins at these ages.
        assert_eq!(choose_topic(&social, &quiet, &dining), 2);
        let mut older = pair(partner, partner, &rows[0], &rows[1], 60, 32, false);
        older.gossip = true;
        // Necken's max age is 59, so Klatsch wins once one of them is 60.
        assert_eq!(choose_topic(&social, &older, &dining), 3);
    }
}

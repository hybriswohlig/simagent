//! "Zeit zu zweit" (ADR-0023, addendum of 2026-11-01; Grok's rounds 43, 44 and 48): a
//! couple's window of month starts in which each attempt has the rate of the template
//! `core:time_together` instead of the adult rate. The window is no state: the world keeps
//! it beside the state like the count of `Start`'s persons, and a load rebuilds it from the
//! input log and the partnerships of the save.

use alloc::collections::BTreeMap;
use alloc::vec::Vec;

use super::partnership::{check_couple, couple_of};
use super::{DomainState, EndedBy, Partnership};
use crate::data::{Data, ShockTemplate};
use crate::data_id::DataId;
use crate::finite::FiniteF64;
use crate::ids::{PartnershipId, SimId};
use crate::input::{Input, InputRecord, InputRejected, RejectCode, TimeTogether};
use crate::log::{Causes, LogPayload, Subject};
use crate::time::{MONTH, SimDuration, SimTime};
use crate::world::SimCtx;

use super::conception::month_start_after;

/// The template of the input.
pub(crate) fn template_id() -> DataId {
    DataId::new("core:time_together").unwrap_or_else(|_| panic!("invariant together.id"))
}

/// The month starts and the rate of the template, if the data hold it as a boost.
pub(crate) fn template(data: &Data) -> Option<(u32, FiniteF64)> {
    data.shock(&template_id()).and_then(ShockTemplate::boost)
}

/// A couple's window: its partnership, its first and its last month start.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Window {
    pub partnership: PartnershipId,
    pub first: SimTime,
    pub last: SimTime,
}

/// The window of an input that acts at `at` for `partnership`: the `months` month starts
/// strictly after `at` (W1); `None` beyond the clock or for no month.
pub(crate) fn window(partnership: PartnershipId, at: SimTime, months: u32) -> Option<Window> {
    let first = month_start_after(at)?;
    let more = MONTH.checked_mul(i64::from(months.checked_sub(1)?))?;
    Some(Window {
        partnership,
        first,
        last: first.checked_add(more)?,
    })
}

/// The rate of an attempt of `partnership` at `t`: `boost` on a month start of the window
/// of that partnership, both ends included, else `adult`.
pub(crate) fn rate_at(
    window: Option<&Window>,
    partnership: PartnershipId,
    t: SimTime,
    adult: f64,
    boost: f64,
) -> f64 {
    match window {
        Some(w) if w.partnership == partnership && w.first <= t && t <= w.last => boost,
        _ => adult,
    }
}

/// Pure check of `TimeTogether` (W2): as a parting (started, a second after the clock, a
/// known and living woman and man, partners of each other), then the template in the data.
pub(crate) fn check(
    domain: &DomainState,
    data: &Data,
    closed_through: SimTime,
    pair: &TimeTogether,
) -> Result<(), InputRejected> {
    check_couple(domain, closed_through, (pair.woman, pair.man))?;
    match template(data) {
        Some(_) => Ok(()),
        None => Err(InputRejected {
            code: RejectCode::ShockUnknown,
        }),
    }
}

/// The window the input opens now for the couple's open partnership; a second one replaces
/// the first (W1: no stacking). Its entry `core:time_together` names the partnership, the
/// woman and the man as subjects, the input as its cause (W4).
pub(crate) fn apply(
    domain: &DomainState,
    data: &Data,
    pair: &TimeTogether,
    ctx: &mut SimCtx<'_, '_>,
) -> Window {
    let id = couple_of(domain, (pair.woman, pair.man))
        .unwrap_or_else(|| panic!("invariant together.checked"));
    let (months, _) = template(data).unwrap_or_else(|| panic!("invariant together.template"));
    let opened =
        window(id, ctx.now(), months).unwrap_or_else(|| panic!("invariant together.window"));
    ctx.emit(
        LogPayload::TimeTogether { partnership: id },
        alloc::vec![Subject::Sim(pair.woman), Subject::Sim(pair.man)],
        Causes::new(ctx.trigger()),
    );
    opened
}

/// Each input of "Zeit zu zweit" of a log over the partnerships of a save (ADR-0023,
/// addendum of 2026-11-01), in the order of the log: the woman, the second it acted in and
/// its window on the partnership it acted on; `None` if one names no partnership of the two
/// that was open when it acted, or the data hold no template. Open when it acted: formed
/// before its second, or in it by an input earlier in the log (a couple of `Start` or a
/// pairing); not ended before its second, nor in it by a parting earlier in the log (an
/// input acts before the events of its second).
pub(crate) fn clicks(
    partnerships: &BTreeMap<PartnershipId, Partnership>,
    log: &[InputRecord],
    data: &Data,
) -> Option<Vec<(SimId, SimTime, Window)>> {
    let after = |record: &InputRecord| record.t.checked_add(SimDuration::from_secs(1));
    let founder = |place: u32| SimId::try_from(u64::from(place) + 1).ok();
    let paired_before = |seq: u64, at: SimTime, couple: (SimId, SimId)| {
        (log.iter()).any(|record| {
            record.seq.get() < seq
                && match &record.input {
                    Input::Start(start) => {
                        at == SimTime::START
                            && (start.couples.iter()).any(|c| {
                                (founder(c.woman), founder(c.man))
                                    == (Some(couple.0), Some(couple.1))
                            })
                    }
                    Input::ArrangePartnership(pair) => {
                        after(record) == Some(at) && (pair.woman, pair.man) == couple
                    }
                    _ => false,
                }
        })
    };
    let parted_before = |seq: u64, at: SimTime, couple: (SimId, SimId)| {
        (log.iter()).any(|record| match &record.input {
            Input::SeparatePartnership(pair) => {
                record.seq.get() < seq
                    && after(record) == Some(at)
                    && (pair.woman, pair.man) == couple
            }
            _ => false,
        })
    };
    let mut clicks = Vec::new();
    for record in log {
        let Input::TimeTogether(pair) = &record.input else {
            continue;
        };
        let (at, seq, couple) = (after(record)?, record.seq.get(), (pair.woman, pair.man));
        let open = |p: &Partnership| {
            let formed = p.formed < at || (p.formed == at && paired_before(seq, at, couple));
            let ended = p.ended.is_some_and(|ending| {
                ending.at < at
                    || (ending.at == at
                        && ending.by == EndedBy::Separation
                        && parted_before(seq, at, couple))
            });
            (p.woman, p.man) == couple && formed && !ended
        };
        let (id, _) = partnerships.iter().find(|(_, p)| open(p))?;
        let (months, _) = template(data)?;
        clicks.push((pair.woman, at, window(*id, at, months)?));
    }
    Some(clicks)
}

/// The windows of a save by woman, the later replacing the earlier (`clicks`).
pub(crate) fn rebuilt(
    partnerships: &BTreeMap<PartnershipId, Partnership>,
    log: &[InputRecord],
    data: &Data,
) -> Option<BTreeMap<SimId, Window>> {
    let clicks = clicks(partnerships, log, data)?;
    Some(
        clicks
            .into_iter()
            .map(|(woman, _, window)| (woman, window))
            .collect(),
    )
}

/// The window that holds for a conception in `partnership` at `t`: the window of the last
/// input on that partnership that acted before `t` (Astra's rounds 1 on the addendum and on
/// slice A: a later input changes no earlier conception, and a new partnership of the same
/// two inherits nothing).
pub(crate) fn window_for(
    clicks: &[(SimId, SimTime, Window)],
    partnership: PartnershipId,
    t: SimTime,
) -> Option<Window> {
    (clicks.iter().rev())
        .find(|(_, at, window)| window.partnership == partnership && *at < t)
        .map(|(_, _, window)| *window)
}

#[cfg(test)]
mod tests {
    use super::*;

    const M: i64 = MONTH.as_secs();

    fn t(secs: i64) -> SimTime {
        SimTime::from_secs(secs)
    }

    fn id(n: u64) -> PartnershipId {
        PartnershipId::try_from(n).expect("valid")
    }

    /// W1 by hand: the three month starts strictly after the second the input acts in; on a
    /// month start itself the next three.
    #[test]
    fn the_window_is_the_next_three_month_starts() {
        let w = window(id(1), t(1_001), 3).expect("a window");
        assert_eq!((w.first, w.last), (t(M), t(3 * M)));
        let w = window(id(1), t(M), 3).expect("a window");
        assert_eq!((w.first, w.last), (t(2 * M), t(4 * M)));
        assert_eq!(window(id(1), t(1), 0), None);
    }

    /// The window of a conception is the last input's on its own partnership before it
    /// (review of slice A): an input of the two on an earlier partnership holds for none.
    #[test]
    fn a_conception_takes_the_window_of_its_own_partnership() {
        let w = |p, at| window(id(p), t(at), 3).expect("a window");
        let clicks = [
            (SimId::try_from(1).expect("valid"), t(1_001), w(1, 1_001)),
            (
                SimId::try_from(1).expect("valid"),
                t(3 * M + 5),
                w(2, 3 * M + 5),
            ),
        ];
        assert_eq!(window_for(&clicks, id(1), t(M)), Some(w(1, 1_001)));
        assert_eq!(window_for(&clicks, id(2), t(M)), None);
        assert_eq!(window_for(&clicks, id(2), t(4 * M)), Some(w(2, 3 * M + 5)));
        assert_eq!(window_for(&clicks, id(1), t(1_001)), None);
    }

    /// The rate inside the window, both ends included, for its own partnership only.
    #[test]
    fn the_rate_is_the_boost_on_the_window_only() {
        let w = window(id(1), t(1_001), 3).expect("a window");
        let rate = |partnership, at| rate_at(Some(&w), id(partnership), t(at), 0.2, 0.3);
        assert_eq!(rate(1, 0), 0.2);
        assert_eq!(rate(1, M), 0.3);
        assert_eq!(rate(1, 2 * M), 0.3);
        assert_eq!(rate(1, 3 * M), 0.3);
        assert_eq!(rate(1, 4 * M), 0.2);
        assert_eq!(rate(2, M), 0.2);
        assert_eq!(rate_at(None, id(1), t(M), 0.2, 0.3), 0.2);
    }
}

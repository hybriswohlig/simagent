//! "Zeit zu zweit" (ADR-0023, addendum of 2026-11-01; Grok's rounds 43, 44 and 48): a
//! couple's window of month starts in which each attempt has the rate of the template
//! `core:time_together` instead of the adult rate. The window is no state: the world keeps
//! it beside the state like the count of `Start`'s persons, and a load rebuilds it from the
//! input log and the partnerships of the save.

use alloc::collections::BTreeMap;

use super::partnership::{check_couple, couple_of};
use super::{DomainState, EndedBy, Partnership};
use crate::data::{Data, ShockTemplate};
use crate::data_id::DataId;
use crate::finite::FiniteF64;
use crate::ids::{PartnershipId, SimId};
use crate::input::{Input, InputRecord, InputRejected, RejectCode, TimeTogether};
use crate::time::{MONTH, SimDuration, SimTime};

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
/// the first (W1: no stacking).
pub(crate) fn opened(
    domain: &DomainState,
    data: &Data,
    pair: &TimeTogether,
    now: SimTime,
) -> Window {
    let id = couple_of(domain, (pair.woman, pair.man))
        .unwrap_or_else(|| panic!("invariant together.checked"));
    let (months, _) = template(data).unwrap_or_else(|| panic!("invariant together.template"));
    window(id, now, months).unwrap_or_else(|| panic!("invariant together.window"))
}

/// The windows of an input log over the partnerships of a save (ADR-0023, addendum of
/// 2026-11-01), by woman, the later replacing the earlier; `None` if an input names no
/// partnership of the two that was open when it acted, or the data hold no template. Open
/// when it acted: formed before its second, or in it by a pairing earlier in the log; not
/// ended before its second, nor in it by a parting earlier in the log (an input acts
/// before the events of its second).
pub(crate) fn rebuilt(
    partnerships: &BTreeMap<PartnershipId, Partnership>,
    log: &[InputRecord],
    data: &Data,
) -> Option<BTreeMap<SimId, Window>> {
    let after = |record: &InputRecord| record.t.checked_add(SimDuration::from_secs(1));
    let earlier = |seq: u64, at: SimTime, couple: (SimId, SimId), pairing: bool| {
        (log.iter()).any(|record| {
            let named = match &record.input {
                Input::ArrangePartnership(pair) if pairing => (pair.woman, pair.man),
                Input::SeparatePartnership(pair) if !pairing => (pair.woman, pair.man),
                _ => return false,
            };
            record.seq.get() < seq && after(record) == Some(at) && named == couple
        })
    };
    let mut windows = BTreeMap::new();
    for record in log {
        let Input::TimeTogether(pair) = &record.input else {
            continue;
        };
        let (at, seq, couple) = (after(record)?, record.seq.get(), (pair.woman, pair.man));
        let open = |p: &Partnership| {
            let formed = p.formed < at || (p.formed == at && earlier(seq, at, couple, true));
            let ended = p.ended.is_some_and(|ending| {
                ending.at < at
                    || (ending.at == at
                        && ending.by == EndedBy::Separation
                        && earlier(seq, at, couple, false))
            });
            (p.woman, p.man) == couple && formed && !ended
        };
        let (id, _) = partnerships.iter().find(|(_, p)| open(p))?;
        let (months, _) = template(data)?;
        windows.insert(pair.woman, window(*id, at, months)?);
    }
    Some(windows)
}

/// The window that holds for a conception of `couple` in `partnership`, formed at `formed`,
/// at `t`: the window of the last input of the two that acted before `t` and in the
/// partnership (Astra's round 1 on the addendum: later inputs change no earlier one).
pub(crate) fn window_for(
    log: &[InputRecord],
    data: &Data,
    couple: (SimId, SimId),
    (partnership, formed): (PartnershipId, SimTime),
    t: SimTime,
) -> Option<Window> {
    let (months, _) = template(data)?;
    let at = (log.iter().rev())
        .filter_map(|record| match &record.input {
            Input::TimeTogether(pair) if (pair.woman, pair.man) == couple => {
                record.t.checked_add(SimDuration::from_secs(1))
            }
            _ => None,
        })
        .find(|at| *at < t)?;
    (at >= formed)
        .then(|| window(partnership, at, months))
        .flatten()
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

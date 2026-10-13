//! The pool file of the pre-run (design note of the pool file; ADR-0020 sections 4 and 5):
//! `cargo xtask prerun pool` runs the pool run on the frozen map, reads the genome keys of
//! its children from the log, and writes the pool (`sim_core::pool`). The frozen
//! calibration carries the frequencies of the seven loci as `pool_frequencies`; they are
//! checked and removed before the data are made, so they enter no data hash.

use std::collections::{BTreeMap, BTreeSet};

use sim_core::data_id::DataId;
use sim_core::pool::{GenomeKeys, Pool, PoolReport, extract};
use sim_core::time::{SimTime, YEAR};
use sim_core::world::{World, YearOut};

use crate::Result;
use crate::ordered::Json;
use crate::prerun::{
    Group, POOL_SEED, data_of, founders, groups, prerun_files, run_observed, start,
};

pub use sim_core::pool::POOL_LOCI;

/// The chromosome each of the seven loci must be on: its rule of inheritance in the pool
/// depends on it (design note, section 3; review).
const CHROMOSOMES: [(&str, &str); 7] = [
    ("core:disease_ar", "autosome"),
    ("core:disease_ad_late", "autosome"),
    ("core:disease_ad_partial", "autosome"),
    ("core:disease_xr", "x"),
    ("core:disease_mt", "mt"),
    ("core:mc1r", "autosome"),
    ("core:herc2", "autosome"),
];

/// The fields of a frequency.
const FIELDS: [&str; 5] = ["value", "unit", "source", "as_of", "note"];

/// The calibration without `pool_frequencies`, and the frequencies (design note, section 3):
/// exactly the seven loci, each a locus of the map, an object of the unit `fraction` with its
/// texts filled and no other field, `0 < q < 1`. The error names the rule.
pub fn take_frequencies(calibration: &Json, map: &Json) -> Result<(Json, BTreeMap<DataId, f64>)> {
    let Json::Object(members) = calibration else {
        return Err("the calibration is no object".to_owned());
    };
    let Some(Json::Object(entry)) = calibration.get("pool_frequencies") else {
        return Err("pool.frequencies_loci: no object pool_frequencies".to_owned());
    };
    let wanted: BTreeSet<&str> = POOL_LOCI.into_iter().collect();
    let given: BTreeSet<&str> = entry.iter().map(|(id, _)| id.as_str()).collect();
    if given != wanted || given.len() != entry.len() {
        return Err(format!("pool.frequencies_loci: {given:?}"));
    }
    let in_map: BTreeSet<&str> = map
        .get("loci")
        .and_then(Json::as_array)
        .map(|loci| {
            loci.iter()
                .filter_map(|locus| locus.get("id").and_then(Json::as_str))
                .collect()
        })
        .unwrap_or_default();
    if !wanted.is_subset(&in_map) {
        return Err("pool.frequencies_loci: a locus is not in the map".to_owned());
    }
    for (id, chromosome) in CHROMOSOMES {
        let on = map
            .get("loci")
            .and_then(Json::as_array)
            .and_then(|loci| {
                loci.iter()
                    .find(|locus| locus.get("id").and_then(Json::as_str) == Some(id))
            })
            .and_then(|locus| locus.get("chromosome"))
            .and_then(Json::as_str);
        if on != Some(chromosome) {
            return Err(format!("pool.frequencies_chromosome {id} on {on:?}"));
        }
    }
    let mut sorted: Vec<&(String, Json)> = entry.iter().collect();
    sorted.sort_by(|a, b| a.0.cmp(&b.0));
    let mut frequencies = BTreeMap::new();
    for (id, value) in sorted {
        if value.get("unit").and_then(Json::as_str) != Some("fraction") {
            return Err(format!("pool.frequency_unit {id}"));
        }
        for name in ["source", "as_of", "note"] {
            let filled = value
                .get(name)
                .and_then(Json::as_str)
                .is_some_and(|text| !text.trim().is_empty());
            if !filled {
                return Err(format!("pool.frequency_text {id} {name}"));
            }
        }
        let Json::Object(fields) = value else {
            return Err(format!("pool.frequencies_loci {id}"));
        };
        let names: BTreeSet<&str> = fields.iter().map(|(name, _)| name.as_str()).collect();
        if names != FIELDS.into_iter().collect() || names.len() != fields.len() {
            return Err(format!("pool.frequencies_loci {id} fields {names:?}"));
        }
        let q = match value.get("value") {
            Some(Json::Number(number)) => number.as_f64(),
            _ => None,
        };
        let Some(q) = q.filter(|q| q.is_finite() && *q > 0.0 && *q < 1.0) else {
            return Err(format!("pool.frequency_range {id}"));
        };
        let locus = DataId::new(id).map_err(|_| format!("pool.frequencies_loci {id}"))?;
        frequencies.insert(locus, q);
    }
    let rest = members
        .iter()
        .filter(|(name, _)| name != "pool_frequencies")
        .cloned()
        .collect();
    Ok((Json::Object(rest), frequencies))
}

/// The pool run on these documents (the frozen map, its calibration with
/// `pool_frequencies`, the life course data) with `founders` founders to `years · YEAR`
/// inclusive, and its pool. `report` sees the run before the extraction.
pub fn pool_of(
    demography: &Json,
    calibration: &Json,
    map: &Json,
    founders_n: u64,
    years: i64,
    report: &mut dyn FnMut(&World, &YearOut, &[Group]) -> Result,
) -> Result<(Pool, PoolReport, GenomeKeys)> {
    let (calibration, frequencies) = take_frequencies(calibration, map)?;
    let frozen = data_of(demography, &calibration, map)?;
    let (run_calibration, run_map) = prerun_files(&calibration, map);
    let data = data_of(demography, &run_calibration, &run_map)?;
    let tag = DataId::new("core:prerun_pool").map_err(|_| "a tag".to_owned())?;
    let groups = groups(&data, founders_n);
    let all = founders(&data, &groups, &tag, POOL_SEED);
    let input = start(&tag, POOL_SEED, all, founders_n);
    let until = SimTime::from_secs(years * YEAR.as_secs());
    let mut keys = GenomeKeys::new();
    let (world, out) = run_observed(data, input, until, &mut |entries| keys.observe(entries))?;
    report(&world, &out, &groups)?;
    let (pool, counted) = extract(&world, &keys, &frozen, &frequencies, POOL_SEED)
        .map_err(|error| format!("the pool: {error:?}"))?;
    Ok((pool, counted, keys))
}

/// What the pool holds (design note, section 3: the alternative alleles per locus; a missing
/// `core:disease_ad_late` is reported, not redrawn).
pub fn print_report(pool: &Pool, report: &PoolReport) {
    println!(
        "pool households {} persons {} unborn {} records {} bytes {} hash {:016x}",
        report.households,
        report.persons,
        report.unborn,
        report.records,
        pool.to_bytes().len(),
        pool.hash()
    );
    for (locus, carriers) in &report.carriers {
        println!("pool carriers {} {carriers}", locus.as_str());
        if locus.as_str() == "core:disease_ad_late" && *carriers == 0 {
            println!("pool report core:disease_ad_late none in the pool (reported, not redrawn)");
        }
    }
}

#[cfg(test)]
#[path = "pool_file_tests.rs"]
mod tests;

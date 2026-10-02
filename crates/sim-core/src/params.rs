//! World parameters (ADR-0004 section 4): set by `Start`, changed only by the override
//! block of a fork (ADR-0001 section 6). They are state and part of the projection.

use serde::{Deserialize, Serialize};

/// The parameters of a world. Every field is required: a scenario names each one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorldParams {
    /// The switch of the hereditary diseases (C5). Off means penetrance 0: the alleles
    /// stay and every random draw is made as with the switch on.
    pub hereditary_diseases: bool,
}

/// One world parameter with its new value. The name of the variant is the data ID of
/// the parameter; new parameters only go to the end.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ParamChange {
    #[serde(rename = "core:hereditary_diseases")]
    HereditaryDiseases(bool),
}

impl WorldParams {
    /// These parameters after `change`.
    pub(crate) const fn with(self, change: ParamChange) -> WorldParams {
        match change {
            ParamChange::HereditaryDiseases(on) => WorldParams {
                hereditary_diseases: on,
            },
        }
    }
}

impl ParamChange {
    /// True if both change the same parameter, whatever the values.
    pub(crate) const fn same_param(self, other: ParamChange) -> bool {
        match (self, other) {
            (ParamChange::HereditaryDiseases(_), ParamChange::HereditaryDiseases(_)) => true,
        }
    }
}

//! The genome of the Durchstich (HANDOFF §9 step 5): the genetic sex and one mendelian
//! dummy locus. The genome type of step 7 takes the place of the genotype.

use serde::{Deserialize, Serialize};

/// The genetic sex.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Sex {
    Female,
    Male,
}

/// An allele of the dummy locus.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Allele {
    Wild,
    Dummy,
}

/// The two alleles of a person at the dummy locus, by the parent they come from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Genotype {
    pub maternal: Allele,
    pub paternal: Allele,
}

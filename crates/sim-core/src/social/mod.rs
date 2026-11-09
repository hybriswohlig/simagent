//! The social layer of phase 3 (ADR-0030, ADR-0031): its data and, so far, the memory of a person
//! in a moved-in household. Bonds, talks and beliefs come with their slices; their values are
//! already part of the data so that the data hash moves once.

pub mod data;
pub mod memory;
pub mod state;

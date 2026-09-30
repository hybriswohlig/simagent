//! `simagent`: command line runner. Loads scenarios, drives the core through its
//! command schema and writes exports. It never decides gameplay (ADR-0000).
//! Arguments use the clap builder API once they exist: clap's derive macros set
//! their own lint levels and collide with the workspace `forbid` lints (ADR-0000).

fn main() {
    println!("simagent {}", env!("CARGO_PKG_VERSION"));
}

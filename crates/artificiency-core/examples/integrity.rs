//! Smoke run for the config integrity guard: prints what the dashboard would
//! show for your real user-level config. Uses a throwaway in-memory store, so
//! every run starts fresh (first pass baselines, second pass is clean).

use artificiency_core::{integrity, Store};

fn main() {
    let store = Store::open_in_memory().expect("store");
    println!("first pass (trust-on-first-use):");
    println!("{:#?}", integrity::check(&store));
    println!("second pass (should be clean):");
    println!("{:#?}", integrity::check(&store));
}

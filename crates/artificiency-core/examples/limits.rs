//! Smoke run for the subscription limits fetch: prints what the sidebar
//! widget would show. Usage: cargo run --example limits

fn main() {
    match artificiency_core::collectors::limits::usage_limits() {
        Some(limits) => println!("{limits:#?}"),
        None => println!("None (no creds / no subscription / fetch failed)"),
    }
}

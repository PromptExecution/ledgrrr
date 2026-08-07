// Not #[cfg(kani)]-gated: downgraded from a symbolic Kani proof to concrete
// #[test]s (see module doc comment) so it runs under plain `cargo test`.
mod invoice_arithmetic;
#[cfg(kani)]
mod vendor_constraints;
// Not #[cfg(kani)]-gated: downgraded from a symbolic Kani proof to concrete
// #[test]s (see module doc comment) so it runs under plain `cargo test`.
mod commit_gate;
#[cfg(kani)]
mod z3_result;
#[cfg(kani)]
mod meta_ctx;

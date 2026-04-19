// Round-trip tests for the process-global log threshold and trace
// flag exposed by `runtime`. These replace the env-var mutation that
// edition 2024 made `unsafe`; regressing them would silently break
// `--trace` and `--log-level`.
//
// Both flags live in `OnceLock`s, so state leaks across tests within
// this binary. All assertions therefore run inside a single `#[test]`
// in a deliberate order.

use keel_lang::runtime;

#[test]
fn log_threshold_and_trace_round_trip() {
    // Log threshold: valid levels take effect; invalid levels are
    // rejected without mutating state.
    assert!(runtime::set_log_threshold("debug"));
    assert_eq!(runtime::current_log_threshold(), 0);

    assert!(runtime::set_log_threshold("warn"));
    assert_eq!(runtime::current_log_threshold(), 2);

    assert!(runtime::set_log_threshold("WARNING")); // case-insensitive alias
    assert_eq!(runtime::current_log_threshold(), 2);

    assert!(!runtime::set_log_threshold("louder"));
    assert_eq!(runtime::current_log_threshold(), 2, "invalid level must not mutate");

    assert!(runtime::set_log_threshold("error"));
    assert_eq!(runtime::current_log_threshold(), 3);

    // Trace flag: toggles, reads reflect the last write.
    runtime::set_trace(true);
    assert!(runtime::trace_enabled());
    runtime::set_trace(false);
    assert!(!runtime::trace_enabled());
}

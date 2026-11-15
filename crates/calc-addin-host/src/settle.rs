//! The settle loop of [`crate::AddinHost::settle`], apart from the Windows shell so that its rule
//! is tested on every platform.
use std::time::{Duration, Instant};

/// Runs `pump` until `signature` has been unchanged for `quiet` (at most `limit`), sleeping
/// `pause` between passes. `raise` (the AfterCalculate the add-ins wait on) runs only for a pass
/// that found the registrations changed since the last pass, and once more when the loop ends if
/// the add-in set registered anything: Excel's recalculation after an add-in has loaded (the
/// startup AfterCalculate the Excel-DNA X0 trace shows after AutoOpen, with no cell changed, and
/// the fifth the original xlOil counts by its first read). An idle add-in set raises nothing per
/// pass: Excel raises AfterCalculate for a calculation, never per message-loop pass (dc-1nbp.3.1).
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn settle_loop(
    quiet: Duration,
    limit: Duration,
    pause: Duration,
    mut signature: impl FnMut() -> u64,
    mut pump: impl FnMut(),
    mut raise: impl FnMut(),
) {
    let start = Instant::now();
    let (mut quiet_since, mut last) = (start, signature());
    while start.elapsed() < limit && quiet_since.elapsed() < quiet {
        pump();
        std::thread::sleep(pause);
        let now = signature();
        if now != last {
            last = now;
            quiet_since = Instant::now();
            raise();
        }
    }
    if last != 0 {
        raise();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    /// An add-in set whose registrations never change raises one (the load's recalculation) however many passes the quiet period takes.
    #[test]
    fn dc_1nbp_3_1__quiet_registrations_raise_one_after_calculate() {
        let (passes, raised) = (Cell::new(0u32), Cell::new(0u32));
        settle_loop(
            Duration::from_millis(60),
            Duration::from_secs(5),
            Duration::from_millis(1),
            || 7,
            || passes.set(passes.get() + 1),
            || raised.set(raised.get() + 1),
        );
        assert!(passes.get() > 10, "the quiet period ran {} passes", passes.get());
        assert_eq!(raised.get(), 1);
    }

    /// An add-in set that registered nothing raises nothing.
    #[test]
    fn dc_1nbp_3_1__no_registrations_raise_nothing() {
        let raised = Cell::new(0u32);
        settle_loop(Duration::from_millis(30), Duration::from_secs(5), Duration::from_millis(1), || 0, || {}, || {
            raised.set(raised.get() + 1)
        });
        assert_eq!(raised.get(), 0);
    }

    /// Each pass that finds a new registration raises one, the loop outlasts the last change, and the load's own recalculation follows.
    #[test]
    fn dc_1nbp_3_1__each_registration_change_raises_one_after_calculate() {
        let (passes, raised) = (Cell::new(0u32), Cell::new(0u32));
        settle_loop(
            Duration::from_millis(40),
            Duration::from_secs(5),
            Duration::from_millis(1),
            || u64::from(passes.get().min(3)),
            || passes.set(passes.get() + 1),
            || raised.set(raised.get() + 1),
        );
        assert_eq!(raised.get(), 4);
    }
}

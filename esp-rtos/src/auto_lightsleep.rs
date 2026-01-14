//! This module implements the automatic light sleep feature.

use core::sync::atomic::Ordering;

use esp_hal::time::Duration;
use macros::BuilderLite;
use portable_atomic::AtomicU32;

use crate::{SCHEDULER, now, scheduler::SchedulerState, task::IdleFn};

cfg_if::cfg_if! {
    if #[cfg(any(esp32c6, esp32h2))] {
        use esp_hal::peripherals::LP_TIMER;
    } else {
        use esp_hal::peripherals::LPWR as LP_TIMER;
    }
}

/// Creates the configuration for the automatic light sleep feature.
///
/// To enable automatic light sleep, call the `enable` method on the returned instance, and use the
/// returned function as the idle hook for
/// [`esp_rtos::start_with_idle_hook`][crate::start_with_idle_hook].
///
/// When the system becomes idle, based on the next expected wakeup time the system will
/// automatically enter light sleep mode. The light sleep can be prevented by peripherals keeping
/// certain clock sources active.
///
/// ## Example
///
/// ```rust,no_run
#[doc = esp_hal::before_snippet!()]
/// use esp_hal::{interrupt::software::SoftwareInterruptControl, timer::timg::TimerGroup};
/// use esp_rtos::AutoLightSleep;
///
/// let timg0 = TimerGroup::new(peripherals.TIMG0);
///
/// #[cfg(any(esp32h2, esp32c6)]
/// let auto_light_sleep_hook = AutoLightSleep::default().enable(peripherals.LP_TIMER);
///
/// #[cfg(not(any(esp32h2, esp32c6))]
/// let auto_light_sleep_hook = AutoLightSleep::default().enable(peripherals.LPWR);
///
/// let software_interrupt = SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
/// esp_rtos::start_with_idle_hook(
///     timg0.timer0,
///     software_interrupt.software_interrupt0,
///     auto_light_sleep_hook,
/// );
#[doc = esp_hal::after_snippet!()]
/// ```
#[derive(BuilderLite, Clone, Copy, PartialEq, Hash)]
pub struct AutoLightSleep {
    /// The minimum sleep time before entering light sleep mode, in microseconds.
    minimum_sleep_time_micros: u32,
}

impl Default for AutoLightSleep {
    fn default() -> Self {
        // TODO: based on the light sleep latency, decide on an actual minimum sleep time
        Self {
            minimum_sleep_time_micros: 1000,
        }
    }
}

fn peripherals_idle() -> bool {
    // TODO introduce a wake lock mechanism, tracking clocks is not enough because the RTOS
    // timebase may be an APB user. Peripherals need to acquire a wake lock only when they
    // are working, not necessarily when they are configured, although this is true for their
    // clocks, too.
    false
}

fn can_enter_light_sleep(expected_sleep_time_us: u64) -> bool {
    expected_sleep_time_us >= MIN_SLEEP_TIME_US.load(Ordering::Relaxed) as u64 && peripherals_idle()
}

extern "C" fn sleep_idle_fn() -> ! {
    // Entering sleep is a longer process and requires a critical section to prevent
    // interrupting it partway through.
    SCHEDULER.with(|scheduler| {
        let next_wakeup_at = unwrap!(scheduler.time_driver.as_ref())
            .timer_queue
            .next_wakeup();
        let expected_sleep_time_us = next_wakeup_at.saturating_sub(now());
        if can_enter_light_sleep(expected_sleep_time_us) {
            todo!()
        }
    });

    crate::task::idle_hook()
}

static MIN_SLEEP_TIME_US: AtomicU32 = AtomicU32::new(0);

impl AutoLightSleep {
    /// Creates the idle hook function for automatic light sleep.
    ///
    /// See the [`AutoLightSleep`] documentation for more information.
    #[must_use = "The returned function must be passed to `esp_rtos::start_with_idle_hook`."]
    pub fn enable(self, _rtc: LP_TIMER<'static>) -> IdleFn {
        sleep_idle_fn
    }

    // TODO: provide an alternative enable function that returns a handle to enter deep sleep.
}

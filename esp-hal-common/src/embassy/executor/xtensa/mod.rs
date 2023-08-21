//! Multicore-aware embassy executors.

#[cfg(feature = "embassy-executor-interrupt")]
pub mod interrupt;

#[cfg(feature = "embassy-executor-interrupt")]
pub use interrupt::*;

#[cfg(feature = "embassy-executor-thread")]
pub mod thread;

#[cfg(feature = "embassy-executor-thread")]
pub use thread::*;

#[export_name = "__pender"]
fn __pender(context: *mut ()) {
    let context = (context as usize).to_le_bytes();

    cfg_if::cfg_if! {
        if #[cfg(feature = "embassy-executor-interrupt")] {
            match context[0] {
                #[cfg(feature = "embassy-executor-thread")]
                0 => pend_thread_mode(context[1] as usize),
                1 => FromCpu1::pend(),
                2 => FromCpu2::pend(),
                3 => FromCpu3::pend(),
                _ => {}
            }
        } else {
            pend_thread_mode(context[1] as usize);
        }
    }
}

#[cfg(feature = "embassy-executor-thread")]
fn pend_thread_mode(core: usize) {
    use core::sync::atomic::Ordering;

    #[cfg(dport)]
    use crate::peripherals::DPORT as SystemPeripheral;
    #[cfg(system)]
    use crate::peripherals::SYSTEM as SystemPeripheral;

    // Signal that there is work to be done.
    SIGNAL_WORK_THREAD_MODE[core].store(true, Ordering::SeqCst);

    // If we are pending a task on the current core, we're done. Otherwise, we
    // need to make sure the other core wakes up.
    #[cfg(multi_core)]
    if core != crate::get_core() as usize {
        // We need to clear the interrupt from software. We don't actually
        // need it to trigger and run the interrupt handler, we just need to
        // kick waiti to return.

        let system = unsafe { &*SystemPeripheral::PTR };
        system
            .cpu_intr_from_cpu_0
            .write(|w| w.cpu_intr_from_cpu_0().bit(true));
    }
}

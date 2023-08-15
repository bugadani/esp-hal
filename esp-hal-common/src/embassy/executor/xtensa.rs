//! Multicore-aware embassy executor.
use core::{
    cell::UnsafeCell,
    marker::PhantomData,
    mem::MaybeUninit,
    ptr,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
};

use embassy_executor::{
    raw::{self, Pender},
    SendSpawner,
    Spawner,
};
#[cfg(esp32)]
use peripherals::DPORT as SystemPeripheral;
#[cfg(not(esp32))]
use peripherals::SYSTEM as SystemPeripheral;

use crate::{get_core, interrupt, peripherals};

/// global atomic used to keep track of whether there is work to do since sev()
/// is not available on Xtensa
#[cfg(not(multi_core))]
static SIGNAL_WORK_THREAD_MODE: [AtomicBool; 1] = [AtomicBool::new(false)];
#[cfg(multi_core)]
static SIGNAL_WORK_THREAD_MODE: [AtomicBool; 2] = [AtomicBool::new(false), AtomicBool::new(false)];

/// Multi-core Xtensa Executor
pub struct Executor {
    inner: raw::Executor,
    not_send: PhantomData<*mut ()>,
}

impl Executor {
    /// Create a new Executor.
    pub fn new() -> Self {
        #[cfg(multi_core)]
        interrupt::enable(
            peripherals::Interrupt::FROM_CPU_INTR0,
            interrupt::Priority::Priority1,
        )
        .unwrap();

        Self {
            inner: raw::Executor::new(Pender::new_from_callback(
                |ctx| {
                    let core = ctx as usize;

                    // Signal that there is work to be done.
                    SIGNAL_WORK_THREAD_MODE[core].store(true, Ordering::SeqCst);

                    // If we are pending a task on the current core, we're done. Otherwise, we
                    // need to make sure the other core wakes up.
                    #[cfg(multi_core)]
                    if core != get_core() as usize {
                        // We need to clear the interrupt from software. We don't actually
                        // need it to trigger and run the interrupt handler, we just need to
                        // kick waiti to return.

                        let system = unsafe { &*SystemPeripheral::PTR };
                        system
                            .cpu_intr_from_cpu_0
                            .write(|w| w.cpu_intr_from_cpu_0().bit(true));
                        system
                            .cpu_intr_from_cpu_0
                            .write(|w| w.cpu_intr_from_cpu_0().bit(false));
                    }
                },
                get_core() as usize as *mut (),
            )),
            not_send: PhantomData,
        }
    }

    /// Run the executor.
    ///
    /// The `init` closure is called with a [`Spawner`] that spawns tasks on
    /// this executor. Use it to spawn the initial task(s). After `init`
    /// returns, the executor starts running the tasks.
    ///
    /// To spawn more tasks later, you may keep copies of the [`Spawner`] (it is
    /// `Copy`), for example by passing it as an argument to the initial
    /// tasks.
    ///
    /// This function requires `&'static mut self`. This means you have to store
    /// the Executor instance in a place where it'll live forever and grants
    /// you mutable access. There's a few ways to do this:
    ///
    /// - a [StaticCell](https://docs.rs/static_cell/latest/static_cell/) (safe)
    /// - a `static mut` (unsafe)
    /// - a local variable in a function you know never returns (like `fn main()
    ///   -> !`), upgrading its lifetime with `transmute`. (unsafe)
    ///
    /// This function never returns.
    pub fn run(&'static mut self, init: impl FnOnce(Spawner)) -> ! {
        init(self.inner.spawner());

        let cpu = get_core() as usize;

        loop {
            unsafe {
                self.inner.poll();

                // Manual critical section implementation that only masks interrupts handlers.
                // We must not acquire the cross-core on dual-core systems because that would
                // prevent the other core from doing useful work while this core is sleeping.
                let token: critical_section::RawRestoreState;
                core::arch::asm!("rsil {0}, 5", out(reg) token);

                // we do not care about race conditions between the load and store operations,
                // interrupts will only set this value to true.
                if SIGNAL_WORK_THREAD_MODE[cpu].load(Ordering::SeqCst) {
                    SIGNAL_WORK_THREAD_MODE[cpu].store(false, Ordering::SeqCst);

                    // if there is work to do, exit critical section and loop back to polling
                    core::arch::asm!(
                    "wsr.ps {0}",
                    "rsync", in(reg) token)
                } else {
                    // waiti sets the PS.INTLEVEL when slipping into sleep
                    // because critical sections in Xtensa are implemented via increasing
                    // PS.INTLEVEL the critical section ends here
                    // take care not add code after `waiti` if it needs to be inside the CS
                    core::arch::asm!("waiti 0"); // critical section ends here
                }
            }
        }
    }
}

pub trait SwPendableInterrupt {
    fn enable(priority: interrupt::Priority);
    fn pend();
    fn clear();
}

macro_rules! from_cpu {
    ($cpu:literal) => {
        paste::paste! {
            pub struct [<FromCpu $cpu>];

            /// Tracks which cores have handled the interrupt. If all cores have, we can clear the
            /// interrupt request.
            #[cfg(multi_core)]
            static [<FROM_CPU_ $cpu _HANDLED>]: AtomicUsize = AtomicUsize::new(0);

            /// The reset value of FROM_CPU_n_HANDLED.
            #[cfg(multi_core)]
            static [<FROM_CPU_ $cpu _ENABLED>]: AtomicUsize = AtomicUsize::new(0);

            impl [<FromCpu $cpu>] {
                fn set_bit(value: bool) {
                    let system = unsafe { &*SystemPeripheral::PTR };
                    system
                        .[<cpu_intr_from_cpu_ $cpu>]
                        .write(|w| w.[<cpu_intr_from_cpu_ $cpu>]().bit(value));
                }
            }

            impl SwPendableInterrupt for [<FromCpu $cpu>] {
                fn enable(priority: interrupt::Priority) {
                    #[cfg(multi_core)]
                    [<FROM_CPU_ $cpu _ENABLED>].fetch_or(1 << get_core() as usize, Ordering::SeqCst);

                    interrupt::enable(peripherals::Interrupt::[<FROM_CPU_INTR $cpu>], priority).unwrap();
                }

                fn pend() {
                    // No matter what core is running this function, we need to pend the interrupt
                    // because that will schedule the executor to run. However, we must only set
                    // the interrupt request in a critical section so that we don't set it while
                    // the other core is trying to clear it.
                    critical_section::with(|_| {
                        #[cfg(multi_core)]
                        [<FROM_CPU_ $cpu _HANDLED>].store([<FROM_CPU_ $cpu _ENABLED>].load(Ordering::SeqCst), Ordering::SeqCst);

                        Self::set_bit(true);
                    });
                }

                fn clear() {
                    // We must only clear the interrupt request when all cores have handled it.
                    // An interrupt may fire at any time, so reading the atomic and clearing the
                    // interrupt request must be done atomically.
                    critical_section::with(|_| {
                        #[cfg(multi_core)]
                        {
                            let cpu_mask = !(1 << get_core() as usize);
                            let old = [<FROM_CPU_ $cpu _HANDLED>].fetch_and(cpu_mask, Ordering::SeqCst);
                            if old != cpu_mask {
                                return;
                            }
                        }
                        Self::set_bit(false);
                    });
                }
            }
        }
    };
}

from_cpu!(1);
from_cpu!(2);
from_cpu!(3);

/// Interrupt mode executor.
///
/// This executor runs tasks in interrupt mode. The interrupt handler is set up
/// to poll tasks, and when a task is woken the interrupt is pended from
/// software.
pub struct InterruptExecutor<SWI>
where
    SWI: SwPendableInterrupt,
{
    core: AtomicUsize,
    executor: UnsafeCell<MaybeUninit<raw::Executor>>,
    _interrupt: PhantomData<SWI>,
}

unsafe impl<SWI: SwPendableInterrupt> Send for InterruptExecutor<SWI> {}
unsafe impl<SWI: SwPendableInterrupt> Sync for InterruptExecutor<SWI> {}

impl<SWI> InterruptExecutor<SWI>
where
    SWI: SwPendableInterrupt,
{
    /// Create a new `InterruptExecutor`.
    #[inline]
    pub const fn new() -> Self {
        Self {
            core: AtomicUsize::new(usize::MAX),
            executor: UnsafeCell::new(MaybeUninit::uninit()),
            _interrupt: PhantomData,
        }
    }

    /// Executor interrupt callback.
    ///
    /// # Safety
    ///
    /// You MUST call this from the interrupt handler, and from nowhere else.
    // TODO: it would be pretty sweet if we could register our own interrupt handler
    // when vectoring is enabled. The user shouldn't need to provide the handler for
    // us.
    pub unsafe fn on_interrupt(&'static self) {
        if !cfg!(multi_core) || get_core() as usize == self.core.load(Ordering::SeqCst) {
            SWI::clear();
            let executor = unsafe { (*self.executor.get()).assume_init_ref() };
            executor.poll();
        }
    }

    /// Start the executor at the given priority level.
    ///
    /// This initializes the executor, enables the interrupt, and returns.
    /// The executor keeps running in the background through the interrupt.
    ///
    /// This returns a [`SendSpawner`] you can use to spawn tasks on it. A
    /// [`SendSpawner`] is returned instead of a [`Spawner`] because the
    /// executor effectively runs in a different "thread" (the interrupt),
    /// so spawning tasks on it is effectively sending them.
    ///
    /// To obtain a [`Spawner`] for this executor, use
    /// [`Spawner::for_current_executor()`] from a task running in it.
    ///
    /// # Interrupt requirements
    ///
    /// You must write the interrupt handler yourself, and make it call
    /// [`Self::on_interrupt()`]
    ///
    /// This method already enables (unmasks) the interrupt, you must NOT do it
    /// yourself.
    ///
    /// You must set the interrupt priority before calling this method. You MUST
    /// NOT do it after.
    ///
    /// [`Spawner`]: embassy_executor::Spawner
    /// [`Spawner::for_current_executor()`]: embassy_executor::Spawner::for_current_executor()
    pub fn start(&'static self, priority: interrupt::Priority) -> SendSpawner {
        if self
            .core
            .compare_exchange(
                usize::MAX,
                get_core() as usize,
                Ordering::Acquire,
                Ordering::Relaxed,
            )
            .is_err()
        {
            panic!("InterruptExecutor::start() called multiple times on the same executor.");
        }

        unsafe {
            (*self.executor.get())
                .as_mut_ptr()
                .write(raw::Executor::new(Pender::new_from_callback(
                    |_| SWI::pend(),
                    ptr::null_mut(),
                )))
        }

        SWI::enable(priority);

        let executor = unsafe { (*self.executor.get()).assume_init_ref() };
        executor.spawner().make_send()
    }
}

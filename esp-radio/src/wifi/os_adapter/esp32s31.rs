use esp_hal::{interrupt, system::Cpu};

use crate::{
    hal::{
        interrupt::Priority,
        peripherals::{Interrupt, WIFI},
        ram,
    },
    interrupt_dispatch::Handler,
    sys::c_types::{c_int, c_void},
};

static ISR_INTERRUPT_0: Handler = Handler::new();
static ISR_INTERRUPT_1: Handler = Handler::new();

pub(crate) fn chip_ints_on(mask: u32) {
    if mask & 1 == 1 {
        interrupt::enable(Interrupt::MODEM_WIFI_PWR, Priority::Priority1);
    }
    if mask & 2 == 2 {
        interrupt::enable(Interrupt::MODEM_WIFI_MAC, Priority::Priority1);
    }
}

pub(crate) fn chip_ints_off(mask: u32) {
    if mask & 1 == 1 {
        interrupt::disable(Cpu::current(), Interrupt::MODEM_WIFI_PWR);
    }
    if mask & 2 == 2 {
        interrupt::disable(Cpu::current(), Interrupt::MODEM_WIFI_MAC);
    }
}

pub(crate) unsafe extern "C" fn set_intr(
    _cpu_no: i32,
    _intr_source: u32,
    _intr_num: u32,
    _intr_prio: i32,
) {
    // These are expected to be direct-bound, but we don't do that for now.
}

pub(crate) unsafe extern "C" fn regdma_link_set_write_wait_content_dummy(
    _arg1: *mut c_void,
    _arg2: u32,
    _arg3: u32,
) {
    todo!()
}

pub(crate) unsafe extern "C" fn sleep_retention_find_link_by_id_dummy(_arg1: c_int) -> *mut c_void {
    todo!()
}

pub unsafe extern "C" fn set_isr(n: i32, f: *mut c_void, arg: *mut c_void) {
    trace!("set_isr - interrupt {} function {:?} arg {:?}", n, f, arg);

    match n {
        0 => ISR_INTERRUPT_0.set(f, arg),
        1 => ISR_INTERRUPT_1.set(f, arg),
        _ => panic!("set_isr - unsupported interrupt number {}", n),
    }
}

#[unsafe(no_mangle)]
#[ram]
extern "C" fn MODEM_WIFI_MAC() {
    ISR_INTERRUPT_1.dispatch();
}

#[unsafe(no_mangle)]
#[ram]
extern "C" fn MODEM_WIFI_PWR() {
    ISR_INTERRUPT_0.dispatch();
}

pub(crate) fn shutdown_wifi_isr() {
    unsafe {
        WIFI::steal().disable_mac_interrupt_on_all_cores();
        WIFI::steal().disable_pwr_interrupt_on_all_cores();
    }
}

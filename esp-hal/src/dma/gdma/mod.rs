//! # General Direct Memory Access (GDMA)
//!
//! ## Overview
//! GDMA is a feature that allows peripheral-to-memory, memory-to-peripheral,
//! and memory-to-memory data transfer at high speed. The CPU is not involved in
//! the GDMA transfer and therefore is more efficient with less workload.
//!
//! The `GDMA` module provides multiple DMA channels, each capable of managing
//! data transfer for various peripherals.
//!
//! Which `DMA_CHn` types are implemented follows device metadata (`dma_engine =
//! "gdma"` peripherals), via `for_each_dma_channel!` from esp-metadata-generated
//! (`GDMA`, …, `(rx, tx)` or `(peri)` IRQ tuples). Each channel row sets `interrupts.peri` for a
//! single RX+TX ISR, or `interrupts.rx` / `interrupts.tx` when the PAC exposes separate lines.

use core::marker::PhantomData;

use crate::{
    dma::*,
    handler,
    interrupt::Priority,
    peripherals::{DMA, Interrupt, pac},
};

#[cfg_attr(dma_gdma_version = "1", path = "ahb_v1.rs")]
#[cfg_attr(dma_gdma_version = "2", path = "ahb_v2.rs")]
mod implementation;

/// An arbitrary GDMA channel
#[derive(Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct AnyGdmaChannel<'d> {
    channel: u8,
    _lifetime: PhantomData<&'d mut ()>,
}

impl AnyGdmaChannel<'_> {
    #[cfg_attr(any(esp32c2, esp32c61), expect(unused))]
    pub(crate) unsafe fn clone_unchecked(&self) -> Self {
        Self {
            channel: self.channel,
            _lifetime: PhantomData,
        }
    }
}

impl crate::private::Sealed for AnyGdmaChannel<'_> {}
impl<'d> DmaChannel for AnyGdmaChannel<'d> {
    type Rx = AnyGdmaRxChannel<'d>;
    type Tx = AnyGdmaTxChannel<'d>;

    unsafe fn split_internal(self, _: crate::private::Internal) -> (Self::Rx, Self::Tx) {
        (
            AnyGdmaRxChannel {
                channel: self.channel,
                _lifetime: PhantomData,
            },
            AnyGdmaTxChannel {
                channel: self.channel,
                _lifetime: PhantomData,
            },
        )
    }
}

/// An arbitrary GDMA RX channel
#[derive(Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct AnyGdmaRxChannel<'d> {
    channel: u8,
    _lifetime: PhantomData<&'d mut ()>,
}

impl<'d> DmaChannelConvert<AnyGdmaRxChannel<'d>> for AnyGdmaRxChannel<'d> {
    fn degrade(self) -> AnyGdmaRxChannel<'d> {
        self
    }
}

/// An arbitrary GDMA TX channel
#[derive(Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct AnyGdmaTxChannel<'d> {
    channel: u8,
    _lifetime: PhantomData<&'d mut ()>,
}

impl<'d> DmaChannelConvert<AnyGdmaTxChannel<'d>> for AnyGdmaTxChannel<'d> {
    fn degrade(self) -> AnyGdmaTxChannel<'d> {
        self
    }
}

use crate::asynch::AtomicWaker;

static TX_WAKERS: [AtomicWaker; CHANNEL_COUNT] = [const { AtomicWaker::new() }; CHANNEL_COUNT];
static RX_WAKERS: [AtomicWaker; CHANNEL_COUNT] = [const { AtomicWaker::new() }; CHANNEL_COUNT];

cfg_if::cfg_if! {
    if #[cfg(any(esp32c2, esp32c3))] {
        use portable_atomic::AtomicBool;
        static TX_IS_ASYNC: [AtomicBool; CHANNEL_COUNT] = [const { AtomicBool::new(false) }; CHANNEL_COUNT];
        static RX_IS_ASYNC: [AtomicBool; CHANNEL_COUNT] = [const { AtomicBool::new(false) }; CHANNEL_COUNT];
    }
}

impl crate::private::Sealed for AnyGdmaTxChannel<'_> {}
impl DmaTxChannel for AnyGdmaTxChannel<'_> {}

impl crate::private::Sealed for AnyGdmaRxChannel<'_> {}
impl DmaRxChannel for AnyGdmaRxChannel<'_> {}

impl<CH: DmaChannel, Dm: DriverMode> Channel<Dm, CH> {
    /// Asserts that the channel is compatible with the given peripheral.
    pub fn runtime_ensure_compatible<P: DmaEligible>(&self, _peripheral: &P) {
        // No runtime checks; GDMA channels are compatible with any peripheral
    }
}

macro_rules! impl_channel {
    ($num:literal, $interrupt_in:ident $(, $interrupt_out:ident)? ) => {
        paste::paste! {
            use $crate::peripherals::[<DMA_CH $num>];
            impl [<DMA_CH $num>]<'_> {
                fn handler_in() -> Option<InterruptHandler> {
                    $crate::if_set! {
                        $({
                            // $interrupt_out is present, meaning we have split handlers
                            #[handler(priority = Priority::max())]
                            fn interrupt_handler_in() {
                                $crate::ignore!($interrupt_out);
                                super::asynch::handle_in_interrupt::<[< DMA_CH $num >]<'static>>();
                            }
                            Some(interrupt_handler_in)
                        })?,
                        {
                            #[handler(priority = Priority::max())]
                            fn interrupt_handler() {
                                super::asynch::handle_in_interrupt::<[< DMA_CH $num >]<'static>>();
                                super::asynch::handle_out_interrupt::<[< DMA_CH $num >]<'static>>();
                            }
                            Some(interrupt_handler)
                        }
                    }
                }

                fn isr_in() -> Option<Interrupt> {
                    Some(Interrupt::$interrupt_in)
                }

                fn handler_out() -> Option<InterruptHandler> {
                    $crate::if_set! {
                        $({
                            #[handler(priority = Priority::max())]
                            fn interrupt_handler_out() {
                                $crate::ignore!($interrupt_out);
                                super::asynch::handle_out_interrupt::<[< DMA_CH $num >]<'static>>();
                            }
                            Some(interrupt_handler_out)
                        })?,
                        None
                    }
                }

                fn isr_out() -> Option<Interrupt> {
                    $crate::if_set! { $(Some(Interrupt::$interrupt_out))?, None }
                }
            }

            impl<'d> DmaChannel for [<DMA_CH $num>]<'d> {
                type Rx = AnyGdmaRxChannel<'d>;
                type Tx = AnyGdmaTxChannel<'d>;

                unsafe fn split_internal(self, _: $crate::private::Internal) -> (Self::Rx, Self::Tx) {
                    (
                        AnyGdmaRxChannel {
                            channel: $num,
                            _lifetime: core::marker::PhantomData,
                        },
                        AnyGdmaTxChannel {
                            channel: $num,
                            _lifetime: core::marker::PhantomData,
                        },
                    )
                }
            }

            impl<'d> DmaChannelConvert<AnyGdmaChannel<'d>> for [<DMA_CH $num>]<'d> {
                fn degrade(self) -> AnyGdmaChannel<'d> {
                    AnyGdmaChannel {
                        channel: $num,
                        _lifetime: core::marker::PhantomData,
                    }
                }
            }

            impl<'d> DmaChannelConvert<AnyGdmaRxChannel<'d>> for [<DMA_CH $num>]<'d> {
                fn degrade(self) -> AnyGdmaRxChannel<'d> {
                    AnyGdmaRxChannel {
                        channel: $num,
                        _lifetime: core::marker::PhantomData,
                    }
                }
            }

            impl<'d> DmaChannelConvert<AnyGdmaTxChannel<'d>> for [<DMA_CH $num>]<'d> {
                fn degrade(self) -> AnyGdmaTxChannel<'d> {
                    AnyGdmaTxChannel {
                        channel: $num,
                        _lifetime: core::marker::PhantomData,
                    }
                }
            }

            impl DmaChannelExt for [<DMA_CH $num>]<'_> {
                fn rx_interrupts() -> impl InterruptAccess<DmaRxInterrupt> {
                    AnyGdmaRxChannel {
                        channel: $num,
                        _lifetime: core::marker::PhantomData,
                    }
                }

                fn tx_interrupts() -> impl InterruptAccess<DmaTxInterrupt> {
                    AnyGdmaTxChannel {
                        channel: $num,
                        _lifetime: core::marker::PhantomData,
                    }
                }
            }
        }
    };
}

const CHANNEL_COUNT: usize = cfg!(soc_has_dma_ch0) as usize
    + cfg!(soc_has_dma_ch1) as usize
    + cfg!(soc_has_dma_ch2) as usize
    + cfg!(soc_has_dma_ch3) as usize
    + cfg!(soc_has_dma_ch4) as usize;

// `for_each_dma_channel!`: match `(GDMA, …)` arms before `(…, ($irq))` so split ISR tuples win
// first.
for_each_dma_channel! {
    (GDMA, $instance:ident, $num:literal, ($rx_isr:ident, $tx_isr:ident)) => {
        impl_channel!($num, $rx_isr, $tx_isr);
    };
    (GDMA, $instance:ident, $num:literal, ($irq:ident)) => {
        impl_channel!($num, $irq);
    };
}

for_each_peripheral! {
    (dma_eligible $(( $peri:ident, $name:ident, $id:literal )),*) => {
        crate::dma::impl_dma_eligible! {
            AnyGdmaChannel {
                $($peri => $name,)*
            }
        }
    };
}

pub(super) fn init_dma_racey() {
    DMA::regs()
        .misc_conf()
        .modify(|_, w| w.ahbm_rst_inter().set_bit());
    DMA::regs()
        .misc_conf()
        .modify(|_, w| w.ahbm_rst_inter().clear_bit());
    DMA::regs().misc_conf().modify(|_, w| w.clk_en().set_bit());

    implementation::setup();
}

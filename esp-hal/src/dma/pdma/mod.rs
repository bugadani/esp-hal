//! # Direct Memory Access
//!
//! ## Overview
//! The `pdma` module is part of the DMA driver of `ESP32` and `ESP32-S2`.
//!
//! This module provides efficient direct data transfer capabilities between
//! peripherals and memory without involving the CPU. It enables bidirectional
//! data transfers through DMA channels, making it particularly useful for
//! high-speed data transfers, such as [SPI] and [I2S] communication.
//!
//! [SPI]: ../spi/index.html
//! [I2S]: ../i2s/index.html

use portable_atomic::AtomicBool;

use crate::{
    DriverMode,
    asynch::AtomicWaker,
    dma::{
        Channel,
        DmaChannel,
        DmaChannelConvert,
        DmaChannelExt,
        DmaEligible,
        DmaPeripheral,
        DmaRxInterrupt,
        DmaTxInterrupt,
        InterruptAccess,
        InterruptHandler,
        RegisterAccess,
    },
    handler,
    interrupt::Priority,
    peripherals::Interrupt,
};

#[cfg(soc_has_dma_copy)]
mod copy;
#[cfg(soc_has_dma_crypto)]
mod crypto;
mod i2s;
mod spi;

#[cfg(soc_has_dma_copy)]
pub use copy::{CopyDmaRxChannel, CopyDmaTxChannel};
#[cfg(soc_has_dma_crypto)]
use crypto::CryptoRegisterBlock;
#[cfg(soc_has_dma_crypto)]
pub use crypto::{CryptoDmaChannel, CryptoDmaRxChannel, CryptoDmaTxChannel};
use i2s::I2sRegisterBlock;
pub use i2s::{AnyI2sDmaChannel, I2sDmaChannel, I2sDmaRxChannel, I2sDmaTxChannel};
use spi::SpiRegisterBlock;
pub use spi::{AnySpiDmaChannel, SpiDmaChannel, SpiDmaRxChannel, SpiDmaTxChannel};

#[doc(hidden)]
pub trait PdmaChannel: crate::private::Sealed {
    type RegisterBlock;

    fn register_block(&self) -> &Self::RegisterBlock;
    fn tx_waker(&self) -> &'static AtomicWaker;
    fn rx_waker(&self) -> &'static AtomicWaker;
    fn is_compatible_with(&self, peripheral: DmaPeripheral) -> bool;

    fn peripheral_interrupt(&self) -> Interrupt;
    fn async_handler(&self) -> InterruptHandler;
    fn rx_async_flag(&self) -> &'static AtomicBool;
    fn tx_async_flag(&self) -> &'static AtomicBool;
}

macro_rules! impl_pdma_channel {
    ($peri:ident, $register_block:ident, $instance:ident, $int:ident, [$($compatible:ident),*]) => {
        paste::paste! {
            use $crate::peripherals::[< $instance >];
            impl<'d> DmaChannel for $instance<'d> {
                type Rx = [<$peri DmaRxChannel>]<'d>;
                type Tx = [<$peri DmaTxChannel>]<'d>;

                unsafe fn split_internal(self, _: $crate::private::Internal) -> (Self::Rx, Self::Tx) { unsafe {
                    (
                        [<$peri DmaRxChannel>](Self::steal().into()),
                        [<$peri DmaTxChannel>](Self::steal().into()),
                    )
                }}
            }

            impl DmaChannelExt for $instance<'_> {
                fn rx_interrupts() -> impl InterruptAccess<DmaRxInterrupt> {
                    [<$peri DmaRxChannel>](unsafe { Self::steal() }.into())
                }
                fn tx_interrupts() -> impl InterruptAccess<DmaTxInterrupt> {
                    [<$peri DmaTxChannel>](unsafe { Self::steal() }.into())
                }
            }

            impl PdmaChannel for $instance<'_> {
                type RegisterBlock = $register_block;

                fn register_block(&self) -> &Self::RegisterBlock {
                    $crate::peripherals::[< $instance >]::regs()
                }
                fn tx_waker(&self) -> &'static AtomicWaker {
                    static WAKER: AtomicWaker = AtomicWaker::new();
                    &WAKER
                }
                fn rx_waker(&self) -> &'static AtomicWaker {
                    static WAKER: AtomicWaker = AtomicWaker::new();
                    &WAKER
                }
                fn is_compatible_with(&self, peripheral: DmaPeripheral) -> bool {
                    let compatible_peripherals = [$(DmaPeripheral::$compatible),*];
                    compatible_peripherals.contains(&peripheral)
                }

                fn peripheral_interrupt(&self) -> Interrupt {
                    Interrupt::$int
                }

                fn async_handler(&self) -> InterruptHandler {
                    #[handler(priority = Priority::max())]
                    pub(crate) fn interrupt_handler() {
                        super::asynch::handle_in_interrupt::<$instance<'static>>();
                        super::asynch::handle_out_interrupt::<$instance<'static>>();
                    }

                    interrupt_handler
                }
                fn rx_async_flag(&self) -> &'static AtomicBool {
                    static FLAG: AtomicBool = AtomicBool::new(false);
                    &FLAG
                }
                fn tx_async_flag(&self) -> &'static AtomicBool {
                    static FLAG: AtomicBool = AtomicBool::new(false);
                    &FLAG
                }
            }

            impl<'d> DmaChannelConvert<[<$peri DmaChannel>]<'d>> for $instance<'d> {
                fn degrade(self) -> [<$peri DmaChannel>]<'d> {
                    self.into()
                }
            }

            impl<'d> DmaChannelConvert<[<$peri DmaRxChannel>]<'d>> for $instance<'d> {
                fn degrade(self) -> [<$peri DmaRxChannel>]<'d> {
                    self.into()
                }
            }

            impl<'d> DmaChannelConvert<[<$peri DmaTxChannel>]<'d>> for $instance<'d> {
                fn degrade(self) -> [<$peri DmaTxChannel>]<'d> {
                    self.into()
                }
            }
        }
    };
}

for_each_pdma_channel! {
    (
        $soc_cfg:ident,
        $instance:ident,
        $family:ident,
        $regs:ident,
        $interrupt:ident,
        [ $( ( $host:ident, $dma_variant:ident ) ),* $(,)? ],
    ) => {
        #[cfg($soc_cfg)]
        paste::paste! {
            impl_pdma_channel!($family, $regs, $instance, $interrupt, [ $($dma_variant),* ]);
            $(
                $crate::dma::impl_dma_eligible!([$instance] $host => $dma_variant);
            )*
        }
    };
}

pub(super) fn init_dma_racey() {
    #[cfg(esp32)]
    {
        // (only) on ESP32 we need to configure DPORT for the SPI DMA channels
        // This assigns the DMA channels to the SPI peripherals, which is more
        // restrictive than necessary but we currently support the same
        // number of SPI peripherals as SPI DMA channels so it's not a big
        // deal.
        use crate::peripherals::DPORT;

        DPORT::regs().spi_dma_chan_sel().modify(|_, w| unsafe {
            w.spi2_dma_chan_sel().bits(1);
            w.spi3_dma_chan_sel().bits(2)
        });
    }
}

impl<CH, Dm> Channel<Dm, CH>
where
    CH: DmaChannel,
    Dm: DriverMode,
{
    /// Asserts that the channel is compatible with the given peripheral.
    #[instability::unstable]
    pub fn runtime_ensure_compatible(&self, peripheral: &impl DmaEligible) {
        assert!(
            self.tx
                .tx_impl
                .is_compatible_with(peripheral.dma_peripheral()),
            "This DMA channel is not compatible with {:?}",
            peripheral.dma_peripheral()
        );
    }
}

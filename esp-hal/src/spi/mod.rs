//! Serial Peripheral Interface (SPI)
//!
//! ## Overview
//! The Serial Peripheral Interface (SPI) is a synchronous serial interface
//! useful for communication with external peripherals.
//!
//! ## Configuration
//! This peripheral is capable of operating in either master or slave mode. For
//! more information on these modes, please refer to the documentation in their
//! respective modules.

use crate::dma::DmaError;

pub mod master;

crate::unstable_module! {
    pub mod slave;
}

/// SPI errors
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[non_exhaustive]
pub enum Error {
    /// Error occurred due to a DMA-related issue.
    #[cfg(feature = "unstable")]
    #[cfg_attr(docsrs, doc(cfg(feature = "unstable")))]
    #[allow(clippy::enum_variant_names, reason = "DMA is unstable")]
    DmaError(DmaError),
    /// Error indicating that the maximum DMA transfer size was exceeded.
    MaxDmaTransferSizeExceeded,
    /// Error indicating that the FIFO size was exceeded during SPI
    /// communication.
    FifoSizeExeeded,
    /// Error indicating that the operation is unsupported by the current
    /// implementation or for the given arguments.
    Unsupported,
    /// An unknown error occurred during SPI communication.
    Unknown,
}

#[doc(hidden)]
#[cfg(feature = "unstable")]
impl From<DmaError> for Error {
    fn from(value: DmaError) -> Self {
        Error::DmaError(value)
    }
}

#[doc(hidden)]
#[cfg(not(feature = "unstable"))]
impl From<DmaError> for Error {
    fn from(_value: DmaError) -> Self {
        Error::Unknown
    }
}

impl embedded_hal::spi::Error for Error {
    fn kind(&self) -> embedded_hal::spi::ErrorKind {
        embedded_hal::spi::ErrorKind::Other
    }
}

/// SPI communication modes, defined by clock polarity (CPOL) and clock phase
/// (CPHA).
///
/// These modes control the clock signal's idle state and when data is sampled
/// and shifted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Mode {
    /// Mode 0 (CPOL = 0, CPHA = 0): Clock is low when idle, data is captured on
    /// the rising edge and propagated on the falling edge.
    _0,
    /// Mode 1 (CPOL = 0, CPHA = 1): Clock is low when idle, data is captured on
    /// the falling edge and propagated on the rising edge.
    _1,
    /// Mode 2 (CPOL = 1, CPHA = 0): Clock is high when idle, data is captured
    /// on the falling edge and propagated on the rising edge.
    _2,
    /// Mode 3 (CPOL = 1, CPHA = 1): Clock is high when idle, data is captured
    /// on the rising edge and propagated on the falling edge.
    _3,
}

/// SPI Bit Order
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum BitOrder {
    /// Most Significant Bit (MSB) is transmitted first.
    MsbFirst,
    /// Least Significant Bit (LSB) is transmitted first.
    LsbFirst,
}

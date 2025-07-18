#![cfg_attr(docsrs, procmacros::doc_replace)]
//! # Parallel Interface (via I2S)
//!
//! ## Overview
//! The I2S parallel interface allows for high-speed data transfer between the
//! ESP32 and external devices. It is commonly used to external devices such as
//! LED matrix, LCD display, and Printer. Only TX is implemented. Each
//! unit can have up to 8 or 16 data signals (depending on your target hardware)
//! plus 1 clock signal.
//!
//! ## Notes
//!
//! Data output is interleaved:
//! - 8bit: [A, B, C, D] is output as [C, D, A, B]  (i.e., swapped as 16bit words)
//! - 16bit: [A, B, C, D] is output as [B, A, D, C] (i.e., 16bit words are swapped)
#![cfg_attr(esp32, doc = "")]
#![cfg_attr(
    esp32,
    doc = "I2S0 does not support true 8bit parallel output, so if you want to do 8bit"
)]
#![cfg_attr(
    esp32,
    doc = "you should use I2S1.  If you have to use I2S0, it will only output the even"
)]
#![cfg_attr(esp32, doc = "bytes! so [A, B, C, D] will be output as [A, C]!!!!")]
#![cfg_attr(esp32, doc = "")]
//! ## Configuration
//!
//! The driver uses DMA (Direct Memory Access) for efficient data transfer and
//! supports various configurations, such as different data formats, standards
//! (e.g., Philips) and pin configurations. It relies on other peripheral
//! modules, such as
//!   - `GPIO`
//!   - `DMA`
//!   - `system` (to configure and enable the I2S peripheral)
//!
//! ## Examples
//!
//! ```rust, no_run
//! # {before_snippet}
//! # use esp_hal::dma::DmaTxBuf;
//! # use esp_hal::dma_buffers;
//! # use esp_hal::delay::Delay;
//! # use esp_hal::i2s::parallel::{I2sParallel, TxEightBits};
//!
//! const BUFFER_SIZE: usize = 256;
//!
//! let delay = Delay::new();
//! let dma_channel = peripherals.DMA_I2S1;
//! let i2s = peripherals.I2S1;
//! let clock = peripherals.GPIO25;
//!
//! let pins = TxEightBits::new(
//!     peripherals.GPIO16,
//!     peripherals.GPIO4,
//!     peripherals.GPIO17,
//!     peripherals.GPIO18,
//!     peripherals.GPIO5,
//!     peripherals.GPIO19,
//!     peripherals.GPIO12,
//!     peripherals.GPIO14,
//! );
//!
//! let (_, _, tx_buffer, tx_descriptors) = dma_buffers!(0, BUFFER_SIZE);
//! let mut parallel =
//!     I2sParallel::new(i2s, dma_channel, Rate::from_mhz(1), pins, clock).into_async();
//!
//! for (i, data) in tx_buffer.chunks_mut(4).enumerate() {
//!     let offset = i * 4;
//!     // i2s parallel driver expects the buffer to be interleaved
//!     data[0] = (offset + 2) as u8;
//!     data[1] = (offset + 3) as u8;
//!     data[2] = offset as u8;
//!     data[3] = (offset + 1) as u8;
//! }
//!
//! let mut tx_buf: DmaTxBuf =
//!     DmaTxBuf::new(tx_descriptors, tx_buffer).expect("DmaTxBuf::new failed");
//!
//! // Sending 256 bytes.
//! loop {
//!     let xfer = match parallel.send(tx_buf) {
//!         Ok(xfer) => xfer,
//!         Err(_) => {
//!             panic!("Failed to send buffer");
//!         }
//!     };
//!     (parallel, tx_buf) = xfer.wait();
//!     delay.delay_millis(10);
//! }
//! # }
//! ```
use core::{
    mem::ManuallyDrop,
    ops::{Deref, DerefMut},
};

use crate::{
    Async,
    Blocking,
    DriverMode,
    dma::{
        Channel,
        ChannelTx,
        DmaChannelFor,
        DmaEligible,
        DmaError,
        DmaTxBuffer,
        PeripheralTxChannel,
        asynch::DmaTxFuture,
    },
    gpio::{
        OutputConfig,
        OutputSignal,
        interconnect::{self, PeripheralOutput},
    },
    i2s::AnyI2s,
    pac::i2s0::RegisterBlock,
    peripherals::{I2S0, I2S1},
    system::PeripheralGuard,
    time::Rate,
};

#[doc(hidden)]
pub trait TxPins<'d> {
    fn bus_width(&self) -> u8;
    fn configure(&mut self, instance: &(impl Instance + 'd));
}

/// Represents a group of 16 output pins configured for 16-bit parallel data
/// transmission.
pub struct TxSixteenBits<'d> {
    pins: [interconnect::OutputSignal<'d>; 16],
}

impl<'d> TxSixteenBits<'d> {
    #[expect(clippy::too_many_arguments)]
    /// Creates a new `TxSixteenBits` instance with the provided output pins.
    pub fn new(
        pin_0: impl PeripheralOutput<'d>,
        pin_1: impl PeripheralOutput<'d>,
        pin_2: impl PeripheralOutput<'d>,
        pin_3: impl PeripheralOutput<'d>,
        pin_4: impl PeripheralOutput<'d>,
        pin_5: impl PeripheralOutput<'d>,
        pin_6: impl PeripheralOutput<'d>,
        pin_7: impl PeripheralOutput<'d>,
        pin_8: impl PeripheralOutput<'d>,
        pin_9: impl PeripheralOutput<'d>,
        pin_10: impl PeripheralOutput<'d>,
        pin_11: impl PeripheralOutput<'d>,
        pin_12: impl PeripheralOutput<'d>,
        pin_13: impl PeripheralOutput<'d>,
        pin_14: impl PeripheralOutput<'d>,
        pin_15: impl PeripheralOutput<'d>,
    ) -> Self {
        Self {
            pins: [
                pin_0.into(),
                pin_1.into(),
                pin_2.into(),
                pin_3.into(),
                pin_4.into(),
                pin_5.into(),
                pin_6.into(),
                pin_7.into(),
                pin_8.into(),
                pin_9.into(),
                pin_10.into(),
                pin_11.into(),
                pin_12.into(),
                pin_13.into(),
                pin_14.into(),
                pin_15.into(),
            ],
        }
    }
}

impl<'d> TxPins<'d> for TxSixteenBits<'d> {
    fn bus_width(&self) -> u8 {
        self.pins.len() as u8
    }

    fn configure(&mut self, instance: &(impl Instance + 'd)) {
        let bits = self.bus_width();
        for (i, pin) in self.pins.iter_mut().enumerate() {
            pin.apply_output_config(&OutputConfig::default());
            pin.set_output_enable(true);
            instance.data_out_signal(i, bits).connect_to(pin);
        }
    }
}

/// Represents a group of 8 output pins configured for 8-bit parallel data
/// transmission.
pub struct TxEightBits<'d> {
    pins: [interconnect::OutputSignal<'d>; 8],
}

impl<'d> TxEightBits<'d> {
    #[expect(clippy::too_many_arguments)]
    /// Creates a new `TxSEightBits` instance with the provided output pins.
    pub fn new(
        pin_0: impl PeripheralOutput<'d>,
        pin_1: impl PeripheralOutput<'d>,
        pin_2: impl PeripheralOutput<'d>,
        pin_3: impl PeripheralOutput<'d>,
        pin_4: impl PeripheralOutput<'d>,
        pin_5: impl PeripheralOutput<'d>,
        pin_6: impl PeripheralOutput<'d>,
        pin_7: impl PeripheralOutput<'d>,
    ) -> Self {
        Self {
            pins: [
                pin_0.into(),
                pin_1.into(),
                pin_2.into(),
                pin_3.into(),
                pin_4.into(),
                pin_5.into(),
                pin_6.into(),
                pin_7.into(),
            ],
        }
    }
}

impl<'d> TxPins<'d> for TxEightBits<'d> {
    fn bus_width(&self) -> u8 {
        self.pins.len() as u8
    }

    fn configure(&mut self, instance: &(impl Instance + 'd)) {
        let bits = self.bus_width();
        for (i, pin) in self.pins.iter_mut().enumerate() {
            pin.apply_output_config(&OutputConfig::default());
            pin.set_output_enable(true);
            instance.data_out_signal(i, bits).connect_to(pin);
        }
    }
}

/// I2S Parallel Interface
pub struct I2sParallel<'d, Dm>
where
    Dm: DriverMode,
{
    instance: AnyI2s<'d>,
    tx_channel: ChannelTx<Dm, PeripheralTxChannel<AnyI2s<'d>>>,
    _guard: PeripheralGuard,
}

impl<'d> I2sParallel<'d, Blocking> {
    /// Create a new I2S Parallel Interface
    pub fn new(
        i2s: impl Instance + 'd,
        channel: impl DmaChannelFor<AnyI2s<'d>>,
        frequency: Rate,
        mut pins: impl TxPins<'d>,
        clock_pin: impl PeripheralOutput<'d>,
    ) -> Self {
        let channel = Channel::new(channel.degrade());
        channel.runtime_ensure_compatible(&i2s);

        let i2s = i2s.degrade();

        let guard = PeripheralGuard::new(i2s.peripheral());

        // configure the I2S peripheral for parallel mode
        i2s.setup(frequency, pins.bus_width());
        // setup the clock pin
        let clock_pin = clock_pin.into();

        clock_pin.apply_output_config(&OutputConfig::default());
        clock_pin.set_output_enable(true);

        i2s.ws_signal().connect_to(&clock_pin);

        pins.configure(&i2s);
        Self {
            instance: i2s,
            tx_channel: channel.tx,
            _guard: guard,
        }
    }

    /// Converts the I2S instance into async mode.
    pub fn into_async(self) -> I2sParallel<'d, Async> {
        I2sParallel {
            instance: self.instance,
            tx_channel: self.tx_channel.into_async(),
            _guard: self._guard,
        }
    }
}

impl<'d> I2sParallel<'d, Async> {
    /// Converts the I2S instance into async mode.
    pub fn into_blocking(self) -> I2sParallel<'d, Blocking> {
        I2sParallel {
            instance: self.instance,
            tx_channel: self.tx_channel.into_blocking(),
            _guard: self._guard,
        }
    }
}

impl<'d, Dm> I2sParallel<'d, Dm>
where
    Dm: DriverMode,
{
    /// Write data to the I2S peripheral
    pub fn send<BUF: DmaTxBuffer>(
        mut self,
        mut data: BUF,
    ) -> Result<I2sParallelTransfer<'d, BUF, Dm>, (DmaError, Self, BUF)> {
        self.instance.tx_reset();
        self.instance.tx_fifo_reset();
        let result = unsafe {
            self.tx_channel
                .prepare_transfer(self.instance.dma_peripheral(), &mut data)
        }
        .and_then(|_| self.tx_channel.start_transfer());
        if let Err(err) = result {
            return Err((err, self, data));
        }
        self.instance.tx_start();
        Ok(I2sParallelTransfer {
            i2s: ManuallyDrop::new(self),
            buf_view: ManuallyDrop::new(data.into_view()),
        })
    }
}

/// Represents an ongoing (or potentially finished) transfer using the i2s
/// parallel interface
pub struct I2sParallelTransfer<'d, BUF, Dm>
where
    BUF: DmaTxBuffer,
    Dm: DriverMode,
{
    i2s: ManuallyDrop<I2sParallel<'d, Dm>>,
    buf_view: ManuallyDrop<BUF::View>,
}

impl<'d, BUF, Dm> I2sParallelTransfer<'d, BUF, Dm>
where
    BUF: DmaTxBuffer,
    Dm: DriverMode,
{
    /// Returns true when [Self::wait] will not block.
    pub fn is_done(&self) -> bool {
        self.i2s.instance.is_tx_done()
    }

    /// Wait for the transfer to finish
    pub fn wait(mut self) -> (I2sParallel<'d, Dm>, BUF) {
        self.i2s.instance.tx_wait_done();
        let i2s = unsafe { ManuallyDrop::take(&mut self.i2s) };
        let view = unsafe { ManuallyDrop::take(&mut self.buf_view) };
        core::mem::forget(self);
        (i2s, BUF::from_view(view))
    }

    fn stop_peripherals(&mut self) {
        self.i2s.instance.tx_stop();
        self.i2s.tx_channel.stop_transfer();
    }
}

impl<BUF> I2sParallelTransfer<'_, BUF, Async>
where
    BUF: DmaTxBuffer,
{
    /// Wait for the transfer to finish
    pub async fn wait_for_done(&mut self) -> Result<(), DmaError> {
        DmaTxFuture::new(&mut self.i2s.tx_channel).await
    }
}

impl<BUF, Dm> Deref for I2sParallelTransfer<'_, BUF, Dm>
where
    BUF: DmaTxBuffer,
    Dm: DriverMode,
{
    type Target = BUF::View;

    fn deref(&self) -> &Self::Target {
        &self.buf_view
    }
}

impl<BUF, Dm> DerefMut for I2sParallelTransfer<'_, BUF, Dm>
where
    BUF: DmaTxBuffer,
    Dm: DriverMode,
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.buf_view
    }
}

impl<BUF, Dm> Drop for I2sParallelTransfer<'_, BUF, Dm>
where
    BUF: DmaTxBuffer,
    Dm: DriverMode,
{
    fn drop(&mut self) {
        self.stop_peripherals();

        // SAFETY: This is Drop, we know that self.i2s and self.buf_view
        // won't be touched again.
        let view = unsafe {
            ManuallyDrop::drop(&mut self.i2s);
            ManuallyDrop::take(&mut self.buf_view)
        };
        let _ = BUF::from_view(view);
    }
}

#[doc(hidden)]
#[derive(Debug)]
pub struct I2sClockDividers {
    pub mclk_divider: u32,
    pub bclk_divider: u32,
    pub denominator: u32,
    pub numerator: u32,
}

fn calculate_clock(sample_rate: Rate, data_bits: u8) -> I2sClockDividers {
    // this loosely corresponds to `i2s_std_calculate_clock` and
    // `i2s_ll_tx_set_mclk` in esp-idf
    //
    // main difference is we are using fixed-point arithmetic here
    // plus adjusted for parallel interface clocking

    let sclk = crate::soc::constants::I2S_SCLK; // for now it's fixed 160MHz and 96MHz (just H2)

    let rate = sample_rate.as_hz();

    let mclk = rate * 2;
    let bclk_divider: u32 = if data_bits == 8 { 2 } else { 1 };
    let mut mclk_divider = sclk / mclk;

    let mut ma: u32;
    let mut mb: u32;
    let mut denominator: u32 = 0;
    let mut numerator: u32 = 0;

    let freq_diff = sclk.abs_diff(mclk * mclk_divider);

    if freq_diff != 0 {
        let decimal = freq_diff as u64 * 10000 / mclk as u64;
        // Carry bit if the decimal is greater than 1.0 - 1.0 / (63.0 * 2) = 125.0 /
        // 126.0
        if decimal > 1250000 / 126 {
            mclk_divider += 1;
        } else {
            let mut min: u32 = !0;

            for a in 2..=crate::i2s::master::I2S_LL_MCLK_DIVIDER_MAX {
                let b = (a as u64) * (freq_diff as u64 * 10000u64 / mclk as u64) + 5000;
                ma = ((freq_diff as u64 * 10000u64 * a as u64) / 10000) as u32;
                mb = (mclk as u64 * (b / 10000)) as u32;

                if ma == mb {
                    denominator = a as u32;
                    numerator = (b / 10000) as u32;
                    break;
                }

                if mb.abs_diff(ma) < min {
                    denominator = a as u32;
                    numerator = b as u32;
                    min = mb.abs_diff(ma);
                }
            }
        }
    }

    I2sClockDividers {
        mclk_divider,
        bclk_divider,
        denominator,
        numerator,
    }
}
#[doc(hidden)]
#[allow(private_bounds)]
pub trait PrivateInstance: DmaEligible {
    fn regs(&self) -> &RegisterBlock;
    fn peripheral(&self) -> crate::system::Peripheral;
    fn ws_signal(&self) -> OutputSignal;
    fn data_out_signal(&self, i: usize, bits: u8) -> OutputSignal;

    fn rx_reset(&self) {
        self.regs().conf().modify(|_, w| w.rx_reset().set_bit());
        self.regs().conf().modify(|_, w| w.rx_reset().clear_bit());
    }

    fn rx_dma_reset(&self) {
        self.regs().lc_conf().modify(|_, w| w.in_rst().set_bit());
        self.regs().lc_conf().modify(|_, w| w.in_rst().clear_bit());
    }

    fn rx_fifo_reset(&self) {
        self.regs()
            .conf()
            .modify(|_, w| w.rx_fifo_reset().set_bit());
        self.regs()
            .conf()
            .modify(|_, w| w.rx_fifo_reset().clear_bit());
    }

    fn tx_reset(&self) {
        self.regs().conf().modify(|_, w| w.tx_reset().set_bit());
        // without this delay starting a subsequent transfer will hang waiting
        // for tx_idle to clear (the transfer does not start).
        // While 20 clocks works for 80MHz cpu but 100 is needed for 240MHz!
        xtensa_lx::timer::delay(100);
        self.regs().conf().modify(|_, w| w.tx_reset().clear_bit());
    }

    fn tx_dma_reset(&self) {
        self.regs().lc_conf().modify(|_, w| w.out_rst().set_bit());
        self.regs().lc_conf().modify(|_, w| w.out_rst().clear_bit());
    }

    fn tx_fifo_reset(&self) {
        self.regs()
            .conf()
            .modify(|_, w| w.tx_fifo_reset().set_bit());
        self.regs()
            .conf()
            .modify(|_, w| w.tx_fifo_reset().clear_bit());
    }

    fn tx_clear_interrupts(&self) {
        self.regs().int_clr().write(|w| {
            w.out_done().clear_bit_by_one();
            w.out_total_eof().clear_bit_by_one()
        });
    }

    fn tx_start(&self) {
        // wait for data to show up in the fifo
        while self.regs().int_raw().read().tx_rempty().bit_is_clear() {
            // wait
        }

        // without this transfers are not reliable!
        xtensa_lx::timer::delay(1);

        self.regs().conf().modify(|_, w| w.tx_start().set_bit());

        while self.regs().state().read().tx_idle().bit_is_set() {
            // wait
        }
    }

    fn tx_stop(&self) {
        self.regs().conf().modify(|_, w| w.tx_start().clear_bit());
    }

    fn is_tx_done(&self) -> bool {
        self.regs().state().read().tx_idle().bit_is_set()
    }

    fn tx_wait_done(&self) {
        while self.regs().state().read().tx_idle().bit_is_clear() {
            // wait
        }

        self.regs().conf().modify(|_, w| w.tx_start().clear_bit());
        self.regs().int_clr().write(|w| {
            w.out_done().clear_bit_by_one();
            w.out_total_eof().clear_bit_by_one()
        });
    }

    fn set_clock(&self, clock_settings: I2sClockDividers) {
        self.regs().clkm_conf().modify(|r, w| unsafe {
            w.bits(r.bits() | (crate::soc::constants::I2S_DEFAULT_CLK_SRC << 21))
            // select PLL_160M
        });

        #[cfg(esp32)]
        self.regs()
            .clkm_conf()
            .modify(|_, w| w.clka_ena().clear_bit());

        self.regs().clkm_conf().modify(|_, w| unsafe {
            w.clk_en().set_bit();
            w.clkm_div_num().bits(clock_settings.mclk_divider as u8)
        });

        self.regs().clkm_conf().modify(|_, w| unsafe {
            w.clkm_div_a().bits(clock_settings.denominator as u8);
            w.clkm_div_b().bits(clock_settings.numerator as u8)
        });

        self.regs().sample_rate_conf().modify(|_, w| unsafe {
            w.tx_bck_div_num().bits(clock_settings.bclk_divider as u8);
            w.rx_bck_div_num().bits(clock_settings.bclk_divider as u8)
        });
    }

    fn setup(&self, frequency: Rate, bits: u8) {
        self.set_clock(calculate_clock(frequency, bits));

        // Initialize I2S dev
        self.rx_reset();
        self.tx_reset();
        self.rx_fifo_reset();
        self.tx_fifo_reset();
        self.rx_dma_reset();
        self.tx_dma_reset();

        // clear all bits and enable lcd mode
        self.regs().conf2().write(|w| {
            // 8 bit mode needs this or it updates on half clocks!
            w.lcd_tx_wrx2_en().bit(bits == 8);
            w.lcd_en().set_bit()
        });

        self.regs().sample_rate_conf().modify(|_, w| unsafe {
            w.rx_bits_mod().bits(bits);
            w.tx_bits_mod().bits(bits)
        });

        self.regs().fifo_conf().write(|w| unsafe {
            w.rx_fifo_mod_force_en().set_bit();
            w.tx_fifo_mod_force_en().set_bit();
            w.rx_fifo_mod().bits(1);
            w.tx_fifo_mod().bits(1);
            w.rx_data_num().bits(32);
            w.tx_data_num().bits(32);
            w.dscr_en().set_bit()
        });

        self.regs().conf1().write(|w| {
            w.tx_stop_en().set_bit();
            w.rx_pcm_bypass().set_bit();
            w.tx_pcm_bypass().set_bit()
        });

        self.regs().conf_chan().write(|w| unsafe {
            w.rx_chan_mod().bits(1);
            w.tx_chan_mod().bits(1)
        });

        self.regs().conf().modify(|_, w| {
            w.rx_mono().set_bit();
            w.tx_mono().set_bit();
            w.rx_right_first().set_bit();
            w.tx_right_first().set_bit()
        });
        self.regs().timing().reset();

        self.regs().pd_conf().modify(|_, w| {
            w.fifo_force_pu().set_bit();
            w.fifo_force_pd().clear_bit()
        });
    }
}

impl PrivateInstance for I2S0<'_> {
    fn regs(&self) -> &RegisterBlock {
        unsafe { &*I2S0::PTR.cast::<RegisterBlock>() }
    }

    fn peripheral(&self) -> crate::system::Peripheral {
        crate::system::Peripheral::I2s0
    }

    fn ws_signal(&self) -> OutputSignal {
        OutputSignal::I2S0O_WS
    }
    fn data_out_signal(&self, i: usize, bits: u8) -> OutputSignal {
        assert!(
            bits == 8 || bits == 16,
            "Number of bits must be 8 or 16, got {}",
            bits
        );

        // signals for 8bit and 16bit both start at an offset of 8 for I2S0
        // https://github.com/espressif/esp-idf/blob/9106c43accd9f5e75379f62f12597677213f5023/components/esp_lcd/i80/esp_lcd_panel_io_i2s.c#L701
        match i + 8 {
            0 => OutputSignal::I2S0O_DATA_0,
            1 => OutputSignal::I2S0O_DATA_1,
            2 => OutputSignal::I2S0O_DATA_2,
            3 => OutputSignal::I2S0O_DATA_3,
            4 => OutputSignal::I2S0O_DATA_4,
            5 => OutputSignal::I2S0O_DATA_5,
            6 => OutputSignal::I2S0O_DATA_6,
            7 => OutputSignal::I2S0O_DATA_7,
            8 => OutputSignal::I2S0O_DATA_8,
            9 => OutputSignal::I2S0O_DATA_9,
            10 => OutputSignal::I2S0O_DATA_10,
            11 => OutputSignal::I2S0O_DATA_11,
            12 => OutputSignal::I2S0O_DATA_12,
            13 => OutputSignal::I2S0O_DATA_13,
            14 => OutputSignal::I2S0O_DATA_14,
            15 => OutputSignal::I2S0O_DATA_15,
            16 => OutputSignal::I2S0O_DATA_16,
            17 => OutputSignal::I2S0O_DATA_17,
            18 => OutputSignal::I2S0O_DATA_18,
            19 => OutputSignal::I2S0O_DATA_19,
            20 => OutputSignal::I2S0O_DATA_20,
            21 => OutputSignal::I2S0O_DATA_21,
            22 => OutputSignal::I2S0O_DATA_22,
            23 => OutputSignal::I2S0O_DATA_23,
            other => panic!("Invalid I2S0 Dout pin {}", other),
        }
    }
}

impl PrivateInstance for I2S1<'_> {
    fn regs(&self) -> &RegisterBlock {
        unsafe { &*I2S1::PTR.cast::<RegisterBlock>() }
    }

    fn peripheral(&self) -> crate::system::Peripheral {
        crate::system::Peripheral::I2s1
    }

    fn ws_signal(&self) -> OutputSignal {
        OutputSignal::I2S1O_WS
    }
    fn data_out_signal(&self, i: usize, bits: u8) -> OutputSignal {
        assert!(
            bits == 8 || bits == 16,
            "Number of bits must be 8 or 16, got {}",
            bits
        );

        // signals for 8bit  start at an offset of  8 for 16bit on I2S1
        let pin_offset = if bits == 16 { 8 } else { 0 };

        match i + pin_offset {
            0 => OutputSignal::I2S1O_DATA_0,
            1 => OutputSignal::I2S1O_DATA_1,
            2 => OutputSignal::I2S1O_DATA_2,
            3 => OutputSignal::I2S1O_DATA_3,
            4 => OutputSignal::I2S1O_DATA_4,
            5 => OutputSignal::I2S1O_DATA_5,
            6 => OutputSignal::I2S1O_DATA_6,
            7 => OutputSignal::I2S1O_DATA_7,
            8 => OutputSignal::I2S1O_DATA_8,
            9 => OutputSignal::I2S1O_DATA_9,
            10 => OutputSignal::I2S1O_DATA_10,
            11 => OutputSignal::I2S1O_DATA_11,
            12 => OutputSignal::I2S1O_DATA_12,
            13 => OutputSignal::I2S1O_DATA_13,
            14 => OutputSignal::I2S1O_DATA_14,
            15 => OutputSignal::I2S1O_DATA_15,
            16 => OutputSignal::I2S1O_DATA_16,
            17 => OutputSignal::I2S1O_DATA_17,
            18 => OutputSignal::I2S1O_DATA_18,
            19 => OutputSignal::I2S1O_DATA_19,
            20 => OutputSignal::I2S1O_DATA_20,
            21 => OutputSignal::I2S1O_DATA_21,
            22 => OutputSignal::I2S1O_DATA_22,
            23 => OutputSignal::I2S1O_DATA_23,
            other => panic!("Invalid I2S1 Dout pin {}", other),
        }
    }
}

impl PrivateInstance for AnyI2s<'_> {
    delegate::delegate! {
        to match &self.0 {
            super::any::Inner::I2s0(i2s) => i2s,
            super::any::Inner::I2s1(i2s) => i2s,
        } {
            fn regs(&self) -> &RegisterBlock;
            fn peripheral(&self) -> crate::system::Peripheral;
            fn ws_signal(&self) -> OutputSignal;
            fn data_out_signal(&self, i: usize, bits: u8) -> OutputSignal ;
        }
    }
}

/// A peripheral singleton compatible with the I2S parallel driver.
pub trait Instance: PrivateInstance + super::any::Degrade {}

impl Instance for I2S0<'_> {}
#[cfg(soc_has_i2s1)]
impl Instance for I2S1<'_> {}
impl Instance for AnyI2s<'_> {}

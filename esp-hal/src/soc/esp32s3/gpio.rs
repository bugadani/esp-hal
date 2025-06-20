//! # GPIO configuration module (ESP32-S3)
//!
//! ## Overview
//!
//! The `GPIO` module provides functions and configurations for controlling the
//! `General Purpose Input/Output` pins on the `ESP32-S3` chip. It allows you to
//! configure pins as inputs or outputs, set their state and read their state.
//!
//! Let's get through the functionality and configurations provided by this GPIO
//! module:
//!   - `io_mux_reg(gpio_num: u8) -> &'static
//!     crate::peripherals::io_mux::GPIO0:`:
//!       * Returns the IO_MUX register for the specified GPIO pin number.
//!   - `gpio_intr_enable(int_enable: bool, nmi_enable: bool) -> u8`:
//!       * This function enables or disables GPIO interrupts and Non-Maskable
//!         Interrupts (NMI). It takes two boolean arguments int_enable and
//!         nmi_enable to control the interrupt and NMI enable settings. The
//!         function returns an u8 value representing the interrupt enable
//!         settings.
//!   - `gpio` block:
//!       * Defines the pin configurations for various GPIO pins. Each line
//!         represents a pin and its associated options such as input/output
//!         mode, analog capability, and corresponding functions.
//!   - `analog` block:
//!       * Block defines the analog capabilities of various GPIO pins. Each
//!         line represents a pin and its associated options such as mux
//!         selection, function selection, and input enable.
//!   - `enum InputSignal`:
//!       * This enumeration defines input signals for the GPIO mux. Each input
//!         signal is assigned a specific value.
//!   - `enum OutputSignal`:
//!       * This enumeration defines output signals for the GPIO mux. Each
//!         output signal is assigned a specific value.
//!
//! This trait provides functions to read the interrupt status and NMI status
//! registers for both the `PRO CPU` and `APP CPU`. The implementation uses the
//! `gpio` peripheral to access the appropriate registers.

use crate::{
    gpio::AlternateFunction,
    pac::io_mux,
    peripherals::{GPIO, IO_MUX},
};

pub(crate) const FUNC_IN_SEL_OFFSET: usize = 0;

pub(crate) type InputSignalType = u16;
pub(crate) type OutputSignalType = u16;
pub(crate) const OUTPUT_SIGNAL_MAX: u16 = 256;
pub(crate) const INPUT_SIGNAL_MAX: u16 = 189;

pub(crate) const ONE_INPUT: u8 = 0x38;
pub(crate) const ZERO_INPUT: u8 = 0x3c;

pub(crate) const GPIO_FUNCTION: AlternateFunction = AlternateFunction::_1;

pub(crate) fn io_mux_reg(gpio_num: u8) -> &'static io_mux::GPIO {
    IO_MUX::regs().gpio(gpio_num as usize)
}

pub(crate) fn gpio_intr_enable(int_enable: bool, nmi_enable: bool) -> u8 {
    int_enable as u8 | ((nmi_enable as u8) << 1)
}

/// Peripheral input signals for the GPIO mux
#[allow(non_camel_case_types, clippy::upper_case_acronyms)]
#[derive(Debug, PartialEq, Copy, Clone)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[doc(hidden)]
pub enum InputSignal {
    SPIQ                    = 0,
    SPID                    = 1,
    SPIHD                   = 2,
    SPIWP                   = 3,
    SPID4                   = 7,
    SPID5                   = 8,
    SPID6                   = 9,
    SPID7                   = 10,
    SPIDQS                  = 11,
    U0RXD                   = 12,
    U0CTS                   = 13,
    U0DSR                   = 14,
    U1RXD                   = 15,
    U1CTS                   = 16,
    U1DSR                   = 17,
    U2RXD                   = 18,
    U2CTS                   = 19,
    U2DSR                   = 20,
    I2S1_MCLK               = 21,
    I2S0O_BCK               = 22,
    I2S0_MCLK               = 23,
    I2S0O_WS                = 24,
    I2S0I_SD                = 25,
    I2S0I_BCK               = 26,
    I2S0I_WS                = 27,
    I2S1O_BCK               = 28,
    I2S1O_WS                = 29,
    I2S1I_SD                = 30,
    I2S1I_BCK               = 31,
    I2S1I_WS                = 32,
    PCNT0_SIG_CH0           = 33,
    PCNT0_SIG_CH1           = 34,
    PCNT0_CTRL_CH0          = 35,
    PCNT0_CTRL_CH1          = 36,
    PCNT1_SIG_CH0           = 37,
    PCNT1_SIG_CH1           = 38,
    PCNT1_CTRL_CH0          = 39,
    PCNT1_CTRL_CH1          = 40,
    PCNT2_SIG_CH0           = 41,
    PCNT2_SIG_CH1           = 42,
    PCNT2_CTRL_CH0          = 43,
    PCNT2_CTRL_CH1          = 44,
    PCNT3_SIG_CH0           = 45,
    PCNT3_SIG_CH1           = 46,
    PCNT3_CTRL_CH0          = 47,
    PCNT3_CTRL_CH1          = 48,
    I2S0I_SD1               = 51,
    I2S0I_SD2               = 52,
    I2S0I_SD3               = 53,
    USB_EXTPHY_VP           = 55,
    USB_EXTPHY_VM           = 56,
    USB_EXTPHY_RCV          = 57,
    USB_OTG_IDDIG           = 58,
    USB_OTG_AVALID          = 59,
    USB_SRP_BVALID          = 60,
    USB_OTG_VBUSVALID       = 61,
    USB_SRP_SESSEND         = 62,
    SPI3_CLK                = 66,
    SPI3_Q                  = 67,
    SPI3_D                  = 68,
    SPI3_HD                 = 69,
    SPI3_WP                 = 70,
    SPI3_CS0                = 71,
    RMT_SIG_0               = 81,
    RMT_SIG_1               = 82,
    RMT_SIG_2               = 83,
    RMT_SIG_3               = 84,
    I2CEXT0_SCL             = 89,
    I2CEXT0_SDA             = 90,
    I2CEXT1_SCL             = 91,
    I2CEXT1_SDA             = 92,
    FSPICLK                 = 101,
    FSPIQ                   = 102,
    FSPID                   = 103,
    FSPIHD                  = 104,
    FSPIWP                  = 105,
    FSPIIO4                 = 106,
    FSPIIO5                 = 107,
    FSPIIO6                 = 108,
    FSPIIO7                 = 109,
    FSPICS0                 = 110,
    TWAI_RX                 = 116,
    SUBSPIQ                 = 120,
    SUBSPID                 = 121,
    SUBSPIHD                = 122,
    SUBSPIWP                = 123,
    CAM_DATA_0              = 133,
    CAM_DATA_1              = 134,
    CAM_DATA_2              = 135,
    CAM_DATA_3              = 136,
    CAM_DATA_4              = 137,
    CAM_DATA_5              = 138,
    CAM_DATA_6              = 139,
    CAM_DATA_7              = 140,
    CAM_DATA_8              = 141,
    CAM_DATA_9              = 142,
    CAM_DATA_10             = 143,
    CAM_DATA_11             = 144,
    CAM_DATA_12             = 145,
    CAM_DATA_13             = 146,
    CAM_DATA_14             = 147,
    CAM_DATA_15             = 148,
    CAM_PCLK                = 149,
    CAM_H_ENABLE            = 150,
    CAM_H_SYNC              = 151,
    CAM_V_SYNC              = 152,
    SUBSPID4                = 155,
    SUBSPID5                = 156,
    SUBSPID6                = 157,
    SUBSPID7                = 158,
    SUBSPIDQS               = 159,
    PWM0_SYNC0              = 160,
    PWM0_SYNC1              = 161,
    PWM0_SYNC2              = 162,
    PWM0_F0                 = 163,
    PWM0_F1                 = 164,
    PWM0_F2                 = 165,
    PWM0_CAP0               = 166,
    PWM0_CAP1               = 167,
    PWM0_CAP2               = 168,
    PWM1_SYNC0              = 169,
    PWM1_SYNC1              = 170,
    PWM1_SYNC2              = 171,
    PWM1_F0                 = 172,
    PWM1_F1                 = 173,
    PWM1_F2                 = 174,
    PWM1_CAP0               = 175,
    PWM1_CAP1               = 176,
    PWM1_CAP2               = 177,
    SDHOST_CCMD_IN_1        = 178,
    SDHOST_CCMD_IN_2        = 179,
    SDHOST_CDATA_IN_10      = 180,
    SDHOST_CDATA_IN_11      = 181,
    SDHOST_CDATA_IN_12      = 182,
    SDHOST_CDATA_IN_13      = 183,
    SDHOST_CDATA_IN_14      = 184,
    SDHOST_CDATA_IN_15      = 185,
    SDHOST_CDATA_IN_16      = 186,
    SDHOST_CDATA_IN_17      = 187,
    SDHOST_DATA_STROBE_1    = 192,
    SDHOST_DATA_STROBE_2    = 193,
    SDHOST_CARD_DETECT_N_1  = 194,
    SDHOST_CARD_DETECT_N_2  = 195,
    SDHOST_CARD_WRITE_PRT_1 = 196,
    SDHOST_CARD_WRITE_PRT_2 = 197,
    SDHOST_CARD_INT_N_1     = 198,
    SDHOST_CARD_INT_N_2     = 199,
    SDHOST_CDATA_IN_20      = 213,
    SDHOST_CDATA_IN_21      = 214,
    SDHOST_CDATA_IN_22      = 215,
    SDHOST_CDATA_IN_23      = 216,
    SDHOST_CDATA_IN_24      = 217,
    SDHOST_CDATA_IN_25      = 218,
    SDHOST_CDATA_IN_26      = 219,
    SDHOST_CDATA_IN_27      = 220,
}

/// Peripheral output signals for the GPIO mux
#[allow(non_camel_case_types, clippy::upper_case_acronyms)]
#[derive(Debug, PartialEq, Copy, Clone)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[doc(hidden)] // TODO connection operations are now public on these, we might want to publish
// them
pub enum OutputSignal {
    SPIQ                       = 0,
    SPID                       = 1,
    SPIHD                      = 2,
    SPIWP                      = 3,
    SPICLK                     = 4,
    SPICS0                     = 5,
    SPICS1                     = 6,
    SPID4                      = 7,
    SPID5                      = 8,
    SPID6                      = 9,
    SPID7                      = 10,
    SPIDQS                     = 11,
    U0TXD                      = 12,
    U0RTS                      = 13,
    U0DTR                      = 14,
    U1TXD                      = 15,
    U1RTS                      = 16,
    U1DTR                      = 17,
    U2TXD                      = 18,
    U2RTS                      = 19,
    U2DTR                      = 20,
    I2S1_MCLK                  = 21,
    I2S0O_BCK                  = 22,
    I2S0_MCLK                  = 23,
    I2S0O_WS                   = 24,
    I2S0O_SD                   = 25,
    I2S0I_BCK                  = 26,
    I2S0I_WS                   = 27,
    I2S1O_BCK                  = 28,
    I2S1O_WS                   = 29,
    I2S1O_SD                   = 30,
    I2S1I_BCK                  = 31,
    I2S1I_WS                   = 32,
    USB_EXTPHY_OEN             = 55,
    USB_EXTPHY_VPO             = 57,
    USB_EXTPHY_VMO             = 58,
    SPI3_CLK                   = 66,
    SPI3_Q                     = 67,
    SPI3_D                     = 68,
    SPI3_HD                    = 69,
    SPI3_WP                    = 70,
    SPI3_CS0                   = 71,
    SPI3_CS1                   = 72,
    LEDC_LS_SIG0               = 73,
    LEDC_LS_SIG1               = 74,
    LEDC_LS_SIG2               = 75,
    LEDC_LS_SIG3               = 76,
    LEDC_LS_SIG4               = 77,
    LEDC_LS_SIG5               = 78,
    LEDC_LS_SIG6               = 79,
    LEDC_LS_SIG7               = 80,
    RMT_SIG_0                  = 81,
    RMT_SIG_1                  = 82,
    RMT_SIG_2                  = 83,
    RMT_SIG_3                  = 84,
    I2CEXT0_SCL                = 89,
    I2CEXT0_SDA                = 90,
    I2CEXT1_SCL                = 91,
    I2CEXT1_SDA                = 92,
    GPIO_SD0                   = 93,
    GPIO_SD1                   = 94,
    GPIO_SD2                   = 95,
    GPIO_SD3                   = 96,
    GPIO_SD4                   = 97,
    GPIO_SD5                   = 98,
    GPIO_SD6                   = 99,
    GPIO_SD7                   = 100,
    FSPICLK                    = 101,
    FSPIQ                      = 102,
    FSPID                      = 103,
    FSPIHD                     = 104,
    FSPIWP                     = 105,
    FSPIIO4                    = 106,
    FSPIIO5                    = 107,
    FSPIIO6                    = 108,
    FSPIIO7                    = 109,
    FSPICS0                    = 110,
    FSPICS1                    = 111,
    FSPICS2                    = 112,
    FSPICS3                    = 113,
    FSPICS4                    = 114,
    FSPICS5                    = 115,
    TWAI_TX                    = 116,
    SUBSPICLK                  = 119,
    SUBSPIQ                    = 120,
    SUBSPID                    = 121,
    SUBSPIHD                   = 122,
    SUBSPIWP                   = 123,
    SUBSPICS0                  = 124,
    SUBSPICS1                  = 125,
    FSPIDQS                    = 126,
    SPI3_CS2                   = 127,
    I2S0O_SD1                  = 128,
    LCD_CS                     = 132,
    LCD_DATA_0                 = 133,
    LCD_DATA_1                 = 134,
    LCD_DATA_2                 = 135,
    LCD_DATA_3                 = 136,
    LCD_DATA_4                 = 137,
    LCD_DATA_5                 = 138,
    LCD_DATA_6                 = 139,
    LCD_DATA_7                 = 140,
    LCD_DATA_8                 = 141,
    LCD_DATA_9                 = 142,
    LCD_DATA_10                = 143,
    LCD_DATA_11                = 144,
    LCD_DATA_12                = 145,
    LCD_DATA_13                = 146,
    LCD_DATA_14                = 147,
    LCD_DATA_15                = 148,
    CAM_CLK                    = 149,
    LCD_H_ENABLE               = 150,
    LCD_H_SYNC                 = 151,
    LCD_V_SYNC                 = 152,
    LCD_DC                     = 153,
    LCD_PCLK                   = 154,
    SUBSPID4                   = 155,
    SUBSPID5                   = 156,
    SUBSPID6                   = 157,
    SUBSPID7                   = 158,
    SUBSPIDQS                  = 159,
    PWM0_0A                    = 160,
    PWM0_0B                    = 161,
    PWM0_1A                    = 162,
    PWM0_1B                    = 163,
    PWM0_2A                    = 164,
    PWM0_2B                    = 165,
    PWM1_0A                    = 166,
    PWM1_0B                    = 167,
    PWM1_1A                    = 168,
    PWM1_1B                    = 169,
    PWM1_2A                    = 170,
    PWM1_2B                    = 171,
    SDHOST_CCLK_OUT_1          = 172,
    SDHOST_CCLK_OUT_2          = 173,
    SDHOST_RST_N_1             = 174,
    SDHOST_RST_N_2             = 175,
    SDHOST_CCMD_OD_PULLUP_EN_N = 176,
    SDIO_TOHOST_INT            = 177,
    SDHOST_CCMD_OUT_1          = 178,
    SDHOST_CCMD_OUT_2          = 179,
    SDHOST_CDATA_OUT_10        = 180,
    SDHOST_CDATA_OUT_11        = 181,
    SDHOST_CDATA_OUT_12        = 182,
    SDHOST_CDATA_OUT_13        = 183,
    SDHOST_CDATA_OUT_14        = 184,
    SDHOST_CDATA_OUT_15        = 185,
    SDHOST_CDATA_OUT_16        = 186,
    SDHOST_CDATA_OUT_17        = 187,
    SDHOST_CDATA_OUT_20        = 213,
    SDHOST_CDATA_OUT_21        = 214,
    SDHOST_CDATA_OUT_22        = 215,
    SDHOST_CDATA_OUT_23        = 216,
    SDHOST_CDATA_OUT_24        = 217,
    SDHOST_CDATA_OUT_25        = 218,
    SDHOST_CDATA_OUT_26        = 219,
    SDHOST_CDATA_OUT_27        = 220,
    GPIO                       = 256,
}

macro_rules! rtcio_analog {
    ($pin_num:expr, $pin_reg:expr, $hold:ident) => {
        paste::paste! {
            impl $crate::gpio::RtcPin for $crate::peripherals::[<GPIO $pin_num>]<'_> {
                fn rtc_number(&self) -> u8 {
                    $pin_num
                }

                /// Set the RTC properties of the pin. If `mux` is true then then pin is
                /// routed to RTC, when false it is routed to IO_MUX.
                fn rtc_set_config(&self, input_enable: bool, mux: bool, func: $crate::gpio::RtcFunction) {
                    enable_iomux_clk_gate();

                    // We need `paste` to rewrite something in each function, so that rustc
                    // doesn't trip over trying to substitute a partial expression as `$pin_reg`
                    $crate::peripherals::[<RTC _IO>]::regs()
                        .$pin_reg.modify(|_,w| unsafe {
                            w.fun_ie().bit(input_enable);
                            w.mux_sel().bit(mux);
                            w.fun_sel().bits(func as u8)
                        });
                }

                fn rtcio_pad_hold(&self, enable: bool) {
                    $crate::peripherals::LPWR::regs()
                        .pad_hold()
                        .modify(|_, w| w.$hold().bit(enable));
                }
            }

            impl $crate::gpio::RtcPinWithResistors for $crate::peripherals::[<GPIO $pin_num>]<'_> {
                fn rtcio_pullup(&self, enable: bool) {
                    $crate::peripherals::[<RTC _IO>]::regs()
                        .$pin_reg.modify(|_, w| w.rue().bit(enable));
                }

                fn rtcio_pulldown(&self, enable: bool) {
                    $crate::peripherals::[<RTC _IO>]::regs()
                        .$pin_reg.modify(|_, w| w.rde().bit(enable));
                }
            }

            impl $crate::gpio::AnalogPin for $crate::peripherals::[<GPIO $pin_num>]<'_> {
                /// Configures the pin for analog mode.
                fn set_analog(&self, _: $crate::private::Internal) {
                    use $crate::gpio::RtcPin;
                    enable_iomux_clk_gate();

                    let rtcio = $crate::peripherals::[<RTC _IO>]::regs();

                    // disable output
                    rtcio.enable_w1tc().write(|w| unsafe { w.enable_w1tc().bits(1 << self.rtc_number()) });

                    // disable open drain
                    rtcio.pin(self.rtc_number() as usize).modify(|_,w| w.pad_driver().bit(false));

                    rtcio.$pin_reg.modify(|_,w| {
                        w.fun_ie().clear_bit();

                        // Connect pin to analog / RTC module instead of standard GPIO
                        w.mux_sel().set_bit();

                        // Select function "RTC function 1" (GPIO) for analog use
                        unsafe { w.fun_sel().bits(0b00) };

                        // Disable pull-up and pull-down resistors on the pin
                        w.rue().bit(false);
                        w.rde().bit(false);

                        w
                    });
                }
            }
        }
    };

    (
        $( ( $pin_num:expr, $pin_reg:expr, $hold:ident ) )+
    ) => {
        $(
            rtcio_analog!($pin_num, $pin_reg, $hold);
        )+
    };
}

rtcio_analog! {
    ( 0, touch_pad(0),   touch_pad0 )
    ( 1, touch_pad(1),   touch_pad1 )
    ( 2, touch_pad(2),   touch_pad2 )
    ( 3, touch_pad(3),   touch_pad3 )
    ( 4, touch_pad(4),   touch_pad4 )
    ( 5, touch_pad(5),   touch_pad5 )
    ( 6, touch_pad(6),   touch_pad6 )
    ( 7, touch_pad(7),   touch_pad7 )
    ( 8, touch_pad(8),   touch_pad8 )
    ( 9, touch_pad(9),   touch_pad9 )
    (10, touch_pad(10),  touch_pad10)
    (11, touch_pad(11),  touch_pad11)
    (12, touch_pad(12),  touch_pad12)
    (13, touch_pad(13),  touch_pad13)
    (14, touch_pad(14),  touch_pad14)
    (15, xtal_32p_pad(), x32p       )
    (16, xtal_32n_pad(), x32n       )
    (17, pad_dac1(),     pdac1      )
    (18, pad_dac2(),     pdac2      )
    (19, rtc_pad19(),    pad19      )
    (20, rtc_pad20(),    pad20      )
    (21, rtc_pad21(),    pad21      )
}

// Whilst the S3 is a dual core chip, it shares the enable registers between
// cores so treat it as a single core device
#[derive(Clone, Copy)]
pub(crate) enum InterruptStatusRegisterAccess {
    Bank0,
    Bank1,
}

impl InterruptStatusRegisterAccess {
    pub(crate) fn interrupt_status_read(self) -> u32 {
        match self {
            Self::Bank0 => GPIO::regs().pcpu_int().read().bits(),
            Self::Bank1 => GPIO::regs().pcpu_int1().read().bits(),
        }
    }
}

// implement marker traits on USB pins
impl crate::otg_fs::UsbDm for crate::peripherals::GPIO19<'_> {}
impl crate::otg_fs::UsbDp for crate::peripherals::GPIO20<'_> {}

fn enable_iomux_clk_gate() {
    crate::peripherals::SENS::regs()
        .sar_peri_clk_gate_conf()
        .modify(|_, w| w.iomux_clk_en().set_bit());
}

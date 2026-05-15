//! Metadata for Espressif devices, primarily intended for use in build scripts.
mod cfg;

use core::str::FromStr;
use std::{fmt::Write, sync::OnceLock};

use anyhow::{Context, Result, bail, ensure};
use cfg::PeriConfig;
use indexmap::IndexMap;
pub use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use strum::IntoEnumIterator;

mod support_status;

use crate::{
    cfg::{SupportItem, Value},
    support_status::SupportStatusLevel,
};

macro_rules! include_toml {
    (Config, $file:expr) => {{
        static LOADED_TOML: OnceLock<Config> = OnceLock::new();
        LOADED_TOML.get_or_init(|| {
            let config: Config = basic_toml::from_str(include_str!($file))
                .with_context(|| format!("Failed to load device configuration: {}", $file))
                .unwrap();

            config
                .validate()
                .with_context(|| format!("Failed to validate device configuration: {}", $file))
                .unwrap();

            config
        })
    }};
}

/// Supported device architectures.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    serde::Deserialize,
    serde::Serialize,
    strum::Display,
    strum::EnumIter,
    strum::EnumString,
    strum::AsRefStr,
)]
#[serde(rename_all = "lowercase")]
#[strum(serialize_all = "lowercase")]
pub enum Arch {
    /// RISC-V architecture
    RiscV,
    /// Xtensa architecture
    Xtensa,
}

/// Device core count.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    serde::Deserialize,
    serde::Serialize,
    strum::Display,
    strum::EnumIter,
    strum::EnumString,
    strum::AsRefStr,
)]
pub enum Cores {
    /// Single CPU core
    #[serde(rename = "single_core")]
    #[strum(serialize = "single_core")]
    Single,
    /// Two or more CPU cores
    #[serde(rename = "multi_core")]
    #[strum(serialize = "multi_core")]
    Multi,
}

/// Supported devices.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    serde::Deserialize,
    serde::Serialize,
    strum::Display,
    strum::EnumIter,
    strum::EnumString,
    strum::AsRefStr,
)]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
#[serde(rename_all = "kebab-case")]
#[strum(serialize_all = "kebab-case")]
pub enum Chip {
    /// ESP32
    Esp32,
    /// ESP32-C2, ESP8684
    Esp32c2,
    /// ESP32-C3, ESP8685
    Esp32c3,
    /// ESP32-C5
    Esp32c5,
    /// ESP32-C6
    Esp32c6,
    /// ESP32-C61
    Esp32c61,
    /// ESP32-H2
    Esp32h2,
    /// ESP32-P4 (chip revision v3.x / eco5 only)
    Esp32p4,
    /// ESP32-S2
    Esp32s2,
    /// ESP32-S3
    Esp32s3,
}

impl Chip {
    pub fn from_cargo_feature() -> Result<Self> {
        let all_chips = Chip::iter().map(|c| c.to_string()).collect::<Vec<_>>();

        let mut chip = None;
        for c in all_chips.iter() {
            if std::env::var(format!("CARGO_FEATURE_{}", c.to_uppercase())).is_ok() {
                if chip.is_some() {
                    bail!(
                        "Expected exactly one of the following features to be enabled: {}",
                        all_chips.join(", ")
                    );
                }
                chip = Some(c);
            }
        }

        let Some(chip) = chip else {
            bail!(
                "Expected exactly one of the following features to be enabled: {}",
                all_chips.join(", ")
            );
        };

        Ok(Self::from_str(chip.as_str()).unwrap())
    }

    pub fn target(&self) -> String {
        Config::for_chip(self).device.target.clone()
    }

    pub fn has_lp_core(&self) -> bool {
        use Chip::*;
        // TODO this should be checking for lp_core_driver_supported
        matches!(self, Esp32c6 | Esp32s2 | Esp32s3)
    }

    pub fn lp_target(&self) -> Result<&'static str> {
        match self {
            Chip::Esp32c5 | Chip::Esp32c6 => Ok("riscv32imac-unknown-none-elf"),
            Chip::Esp32s2 | Chip::Esp32s3 => Ok("riscv32imc-unknown-none-elf"),
            _ => bail!("Chip does not contain an LP core: '{self}'"),
        }
    }

    pub fn name(&self) -> &str {
        match self {
            Chip::Esp32 => "Esp32",
            Chip::Esp32c2 => "Esp32c2",
            Chip::Esp32c3 => "Esp32c3",
            Chip::Esp32c5 => "Esp32c5",
            Chip::Esp32c6 => "Esp32c6",
            Chip::Esp32c61 => "Esp32c61",
            Chip::Esp32h2 => "Esp32h2",
            Chip::Esp32p4 => "Esp32p4",
            Chip::Esp32s2 => "Esp32s2",
            Chip::Esp32s3 => "Esp32s3",
        }
    }

    pub fn pretty_name(&self) -> &str {
        match self {
            Chip::Esp32 => "ESP32",
            Chip::Esp32c2 => "ESP32-C2",
            Chip::Esp32c3 => "ESP32-C3",
            Chip::Esp32c5 => "ESP32-C5",
            Chip::Esp32c6 => "ESP32-C6",
            Chip::Esp32c61 => "ESP32-C61",
            Chip::Esp32h2 => "ESP32-H2",
            Chip::Esp32p4 => "ESP32-P4",
            Chip::Esp32s2 => "ESP32-S2",
            Chip::Esp32s3 => "ESP32-S3",
        }
    }

    pub fn is_xtensa(&self) -> bool {
        matches!(self, Chip::Esp32 | Chip::Esp32s2 | Chip::Esp32s3)
    }

    pub fn is_riscv(&self) -> bool {
        !self.is_xtensa()
    }

    pub fn list_of_possible_symbols() -> &'static IndexMap<String, Option<Vec<String>>> {
        type SymbolMap = IndexMap<String, Option<Vec<String>>>;
        static CACHED_SYMBOLS: OnceLock<SymbolMap> = OnceLock::new();
        CACHED_SYMBOLS.get_or_init(|| {
            let mut cfgs: SymbolMap = SymbolMap::new();

            for chip in Chip::iter() {
                let config = Config::for_chip(&chip);
                for symbol in config.all() {
                    if let Some((symbol_name, symbol_value)) = symbol.split_once('=') {
                        let symbol_name = symbol_name.replace('.', "_");
                        let entry = cfgs.entry(symbol_name).or_default();
                        let vec = entry.get_or_insert_with(Vec::new);

                        // Avoid duplicates in the same cfg.
                        if !vec.contains(&symbol_value.to_string()) {
                            vec.push(symbol_value.to_string());
                        }
                    } else {
                        // https://doc.rust-lang.org/cargo/reference/build-scripts.html#rustc-check-cfg
                        let cfg = symbol.replace('.', "_");

                        if !cfgs.contains_key(&cfg) {
                            cfgs.insert(cfg, None);
                        }
                    }
                }
            }

            cfgs
        })
    }

    pub fn list_of_check_cfgs() -> Vec<String> {
        let mut cfgs = vec![];

        // Used by our documentation builds to prevent the huge red warning banner.
        cfgs.push(String::from("cargo:rustc-check-cfg=cfg(not_really_docsrs)"));
        cfgs.push(String::from("cargo:rustc-check-cfg=cfg(semver_checks)"));

        let possible_symbols = Self::list_of_possible_symbols();
        for (sym, values) in possible_symbols.iter() {
            if values.is_none() {
                cfgs.push(format!("cargo:rustc-check-cfg=cfg({})", sym));
            }
        }

        for (sym, values) in possible_symbols.iter() {
            if let Some(values) = values {
                cfgs.push(format!(
                    "cargo:rustc-check-cfg=cfg({sym}, values({}))",
                    values.join(",")
                ));
            }
        }

        cfgs
    }
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct MemoryRegion {
    name: String,
    start: u32,
    end: u32,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct PeripheralDef {
    /// The name of the esp-hal peripheral singleton
    name: String,
    /// When omitted, same as `name`
    #[serde(default, rename = "pac")]
    pac_name: Option<String>,
    /// Whether or not the peripheral has a PAC counterpart
    #[serde(default, rename = "virtual")]
    is_virtual: bool,
    /// Related PAC interrupt names keyed by convention.
    ///
    /// For PDMA channel peripherals (`dma_engine` other than `"gdma"`), use `dma` or `peri`, etc.
    /// For GDMA channel peripherals (`DMA_CHn` with `dma_engine = "gdma"`), use **`peri`** when
    /// one interrupt covers both RX and TX, or **`rx`** and **`tx`** when the PAC has separate
    /// ISRs.
    #[serde(default)]
    interrupts: IndexMap<String, String>,
    /// Declares which DMA engine backs this peripheral (`dma_engine` id: `gdma` on GDMA SoCs,
    /// `spi` / `i2s` / … on PDMA).
    #[serde(default)]
    dma_user: Option<DmaUser>,
    /// Set to true to hide a peripheral from the Peripherals struct.
    #[serde(default)]
    hidden: bool,
    /// If the peripheral isn't specifically named by a driver, this field can be used to mark it
    /// as stable.
    #[serde(default)]
    stable: bool,

    /// Instantiates a clock group for the peripheral.
    #[serde(default)]
    clock_group: Option<String>,

    /// DMA channel peripheral and engine label: on PDMA (`DMA_SPI2`, …) use ids like `"spi"` /
    /// `"i2s"` / `"crypto"` (same string as host [`DmaUser::engine`]; drives PDMA
    /// [`for_each_pdma_channel!`] types such as `{Pascal}RegisterBlock`). On GDMA (`DMA_CH0`,
    /// …) use `"gdma"` and set [`PeripheralDef::interrupts`] (`peri`, or `rx` + `tx`);
    /// enumerated for `for_each_gdma_channel!` in esp-metadata-generated / esp-hal.
    #[serde(default)]
    dma_engine: Option<DmaEngine>,
}

/// Host DMA routing: [`DmaUser::engine`] matches a channel peripheral's [`DmaEngine`] string
/// (`gdma`, `spi`, …); `peripheral_id` is the hardware selector.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct DmaUser {
    pub engine: String,
    pub peripheral_id: u32,
}

/// `DMA_SPI2` → `SPI2`. Used to pair a PDMA channel singleton with its host when several channels
/// share one engine id (`dma_engine` string).
fn pdma_channel_host_suffix(channel_name: &str) -> &str {
    channel_name.strip_prefix("DMA_").unwrap_or(channel_name)
}

/// Host peripherals served by this PDMA channel: same engine id (`dma_engine`) as
/// [`DmaUser::engine`], and either the host name matches [`pdma_channel_host_suffix`] (e.g.
/// `DMA_SPI2` ↔ `SPI2`) or, if no such host exists, every host with that family (e.g. `DMA_CRYPTO`
/// ↔ `AES` + `SHA`).
fn dma_user_hosts_for_pdma_channel(
    peripherals: &[PeripheralDef],
    channel_peri: &PeripheralDef,
    channel_engine: &DmaEngine,
) -> Vec<String> {
    let Ok(fam_key) = dma_engine_family_key(channel_engine.as_str()) else {
        return Vec::new();
    };
    let suffix = pdma_channel_host_suffix(&channel_peri.name);

    let family_hosts: Vec<&PeripheralDef> = peripherals
        .iter()
        .filter(|p| {
            p.dma_user.as_ref().is_some_and(|u| {
                dma_engine_family_key(&u.engine).ok().as_deref() == Some(fam_key.as_str())
            })
        })
        .collect();

    let exact: Vec<&PeripheralDef> = family_hosts
        .iter()
        .copied()
        .filter(|p| p.name.eq_ignore_ascii_case(suffix))
        .collect();

    let selected = if !exact.is_empty() {
        exact
    } else {
        family_hosts
    };

    let mut hosts: Vec<String> = selected.iter().map(|p| p.name.clone()).collect();
    hosts.sort();
    hosts
}

/// Channel [`DmaEngine`] string: PDMA ids drive [`for_each_pdma_channel!`]; `"gdma"` on `DMA_CH*`
/// rows drives `for_each_gdma_channel!` in esp-hal.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(transparent)]
pub struct DmaEngine(String);

impl DmaEngine {
    /// Lowercase engine id (same string as host [`DmaUser::engine`]). PDMA ids feed
    /// [`for_each_pdma_channel!`]; `"gdma"` feeds `for_each_gdma_channel!`.
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

/// `Interrupt::…` token for PDMA codegen, from [`PeripheralDef::interrupts`].
fn dma_engine_interrupt_name(peri: &PeripheralDef) -> Option<&str> {
    if let Some(v) = peri.interrupts.get("dma") {
        return Some(v.as_str());
    }
    if let Some(v) = peri.interrupts.get("peri") {
        return Some(v.as_str());
    }
    if peri.interrupts.len() == 1 {
        return peri.interrupts.values().next().map(|s| s.as_str());
    }
    None
}

/// Normalized engine id (`dma_engine` / [`DmaUser::engine`]): trimmed, lowercase ASCII `[a-z0-9_]`.
fn dma_engine_family_key(engine_family: &str) -> Result<String> {
    let trimmed = engine_family.trim();
    ensure!(!trimmed.is_empty(), "dma_engine must not be empty");
    for ch in trimmed.chars() {
        ensure!(
            ch.is_ascii_alphanumeric() || ch == '_',
            "dma_engine {:?} may only contain ASCII letters, digits, and underscores",
            engine_family,
        );
    }
    Ok(trimmed.to_ascii_lowercase())
}

fn chip_has_gdma_controller(peri_cfg: &PeriConfig) -> bool {
    peri_cfg
        .dma
        .as_ref()
        .is_some_and(|d| d.kind.eq_ignore_ascii_case("gdma"))
}

/// `DMA_CH0` → `0`. Required for peripherals with [`DmaEngine`] `"gdma"`.
fn gdma_channel_index(peri_name: &str) -> Result<u32> {
    let rest = peri_name.strip_prefix("DMA_CH").with_context(|| {
        format!(
            "{peri_name:?}: dma_engine \"gdma\" peripherals must be named DMA_CH followed by digits (e.g. DMA_CH0)",
        )
    })?;
    let idx = rest
        .parse::<u32>()
        .with_context(|| format!("{peri_name:?}: expected numeric suffix after DMA_CH"))?;
    Ok(idx)
}

/// GDMA `DMA_CHn` row: either one merged ISR (`Interrupt::…` name in **`peri`**) or separate
/// **`rx`** / **`tx`** names.
enum GdmaChannelIrqs<'a> {
    Peri(&'a str),
    RxTx { rx: &'a str, tx: &'a str },
}

fn parse_gdma_channel_interrupts(peri: &PeripheralDef) -> Result<GdmaChannelIrqs<'_>> {
    for k in peri.interrupts.keys() {
        ensure!(
            matches!(k.as_str(), "peri" | "rx" | "tx"),
            "{}: dma_engine \"gdma\" channel `interrupts` keys must be only \"peri\", \"rx\", or \"tx\" (got {:?})",
            peri.name,
            k,
        );
    }
    let peri_irq = peri.interrupts.get("peri").map(|s| s.as_str());
    let rx = peri.interrupts.get("rx").map(|s| s.as_str());
    let tx = peri.interrupts.get("tx").map(|s| s.as_str());
    match (peri_irq, rx, tx) {
        (Some(p), None, None) => {
            ensure!(
                !p.is_empty(),
                "{}: interrupts.peri must not be empty",
                peri.name,
            );
            Ok(GdmaChannelIrqs::Peri(p))
        }
        (None, Some(r), Some(t)) => {
            ensure!(
                !r.is_empty() && !t.is_empty(),
                "{}: interrupts.rx and interrupts.tx must not be empty",
                peri.name,
            );
            Ok(GdmaChannelIrqs::RxTx { rx: r, tx: t })
        }
        (Some(_), Some(_), _) | (Some(_), _, Some(_)) => bail!(
            "{}: use either interrupts.peri alone or interrupts.rx + interrupts.tx, not both",
            peri.name,
        ),
        _ => bail!(
            "{}: dma_engine \"gdma\" requires interrupts.peri (single channel ISR) or interrupts.rx + interrupts.tx (split RX/TX ISRs)",
            peri.name,
        ),
    }
}

/// One lowercase segment of `_`-split engine id → Pascal fragment (`i2s` → `I2s`).
fn dma_engine_family_segment_pascal(seg: &str) -> Result<String> {
    ensure!(!seg.is_empty(), "dma_engine has an empty '_' segment");
    ensure!(
        seg.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()),
        "{seg:?}: dma_engine segments must be lowercase ASCII after normalization",
    );

    let mut chars = seg.chars();
    let Some(head) = chars.next() else {
        unreachable!("validated non-empty");
    };
    ensure!(
        head.is_ascii_alphabetic(),
        "{seg:?}: each '_' segment must begin with an ASCII letter",
    );

    let mut out = String::new();
    out.push(head.to_ascii_uppercase());
    for c in chars {
        if c.is_ascii_alphabetic() {
            out.push(c);
        } else {
            ensure!(
                c.is_ascii_digit(),
                "{seg:?}: invalid character after normalization"
            );
            out.push(c);
        }
    }
    Ok(out)
}

/// Full Pascal form of the engine id for `{Family}RegisterBlock` / `Any{Family}`.
fn dma_engine_family_pascal(family_key: &str) -> Result<String> {
    let segments: Vec<&str> = family_key.split('_').filter(|s| !s.is_empty()).collect();
    ensure!(!segments.is_empty(), "dma_engine is empty");

    let mut out = String::new();
    for seg in segments {
        out.push_str(&dma_engine_family_segment_pascal(seg)?);
    }
    Ok(out)
}

fn peripheral_dma_variant_ident(name: &str) -> proc_macro2::Ident {
    use convert_case::{Boundary, Case, Casing, pattern};
    format_ident!(
        "{}",
        name.from_case(Case::Custom {
            boundaries: &[Boundary::LOWER_UPPER, Boundary::UNDERSCORE],
            pattern: pattern::capital,
            delim: "",
        })
        .to_case(Case::Pascal)
    )
}

impl PeripheralDef {
    fn symbol_name(&self) -> String {
        format!("soc_has_{}", self.name.to_lowercase())
    }
}

#[derive(Debug, Clone, serde::Deserialize)]
struct Device {
    name: String,
    arch: Arch,
    target: String,
    cores: usize,
    trm: String,

    symbols: Vec<String>,

    // Peripheral driver configuration:
    #[serde(flatten)]
    peri_config: PeriConfig,
}

// Output a Display-able value as a TokenStream, intended to generate numbers
// without the type suffix.
fn number(n: impl std::fmt::Display) -> TokenStream {
    TokenStream::from_str(&format!("{n}")).unwrap()
}
fn number_hex(n: impl std::fmt::Display + std::fmt::UpperHex) -> TokenStream {
    TokenStream::from_str(&format!("{n:#X}")).unwrap()
}

/// Device configuration file format.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct Config {
    device: Device,
    #[serde(skip)]
    all_symbols: OnceLock<Vec<String>>,
}

impl Config {
    /// The configuration for the specified chip.
    pub fn for_chip(chip: &Chip) -> &Self {
        match chip {
            Chip::Esp32 => include_toml!(Config, "../devices/esp32.toml"),
            Chip::Esp32c2 => include_toml!(Config, "../devices/esp32c2.toml"),
            Chip::Esp32c3 => include_toml!(Config, "../devices/esp32c3.toml"),
            Chip::Esp32c5 => include_toml!(Config, "../devices/esp32c5.toml"),
            Chip::Esp32c6 => include_toml!(Config, "../devices/esp32c6.toml"),
            Chip::Esp32c61 => include_toml!(Config, "../devices/esp32c61.toml"),
            Chip::Esp32h2 => include_toml!(Config, "../devices/esp32h2.toml"),
            Chip::Esp32p4 => include_toml!(Config, "../devices/esp32p4.toml"),
            Chip::Esp32s2 => include_toml!(Config, "../devices/esp32s2.toml"),
            Chip::Esp32s3 => include_toml!(Config, "../devices/esp32s3.toml"),
        }
    }

    /// Create an empty configuration
    pub fn empty() -> Self {
        Self {
            device: Device {
                name: String::new(),
                arch: Arch::RiscV,
                target: String::new(),
                cores: 1,
                trm: String::new(),
                symbols: Vec::new(),
                peri_config: PeriConfig::default(),
            },
            all_symbols: OnceLock::new(),
        }
    }

    fn validate(&self) -> Result<()> {
        for instance in self.device.peri_config.driver_instances() {
            let (driver, peri) = instance.split_once('.').unwrap();
            ensure!(
                self.peripherals()
                    .iter()
                    .any(|p| p.name.eq_ignore_ascii_case(peri)),
                "Driver {driver} marks an implementation for '{peri}' but this peripheral is not defined for '{}'",
                self.device.name
            );
        }

        let peri_list = self.peripherals();

        for p in peri_list.iter() {
            let Some(u) = &p.dma_user else {
                continue;
            };
            let fk = dma_engine_family_key(&u.engine).with_context(|| {
                format!(
                    "{}: dma_user.engine {:?} must be a valid dma_engine id (ASCII letters, digits, underscores; normalized to lowercase)",
                    p.name, u.engine
                )
            })?;

            if fk == "gdma" {
                if chip_has_gdma_controller(&self.device.peri_config) {
                    let has_channel = peri_list.iter().any(|c| {
                        c.dma_engine.as_ref().is_some_and(|e| {
                            dma_engine_family_key(e.as_str()).ok().as_deref() == Some("gdma")
                        })
                    });
                    ensure!(
                        has_channel,
                        "{}: dma_user.engine {:?} requires at least one peripheral with dma_engine = \"gdma\" (e.g. DMA_CH0)",
                        p.name,
                        u.engine,
                    );
                }
                continue;
            }

            let accepting: Vec<&str> = peri_list
                .iter()
                .filter(|c| {
                    c.dma_engine.as_ref().is_some_and(|eng| {
                        dma_engine_family_key(eng.as_str()).ok().as_deref() == Some(fk.as_str())
                            && dma_user_hosts_for_pdma_channel(peri_list, c, eng)
                                .iter()
                                .any(|h| h.eq_ignore_ascii_case(p.name.as_str()))
                    })
                })
                .map(|c| c.name.as_str())
                .collect();
            ensure!(
                accepting.len() == 1,
                "{}: dma_user (engine {:?}, peripheral_id {}) must route to exactly one PDMA dma_engine peripheral; matched {:?}",
                p.name,
                u.engine,
                u.peripheral_id,
                accepting,
            );
        }

        for p in peri_list.iter() {
            let Some(engine) = &p.dma_engine else {
                continue;
            };
            let fam_key = dma_engine_family_key(engine.as_str())?;

            if fam_key == "gdma" {
                gdma_channel_index(&p.name)?;
                ensure!(
                    chip_has_gdma_controller(&self.device.peri_config),
                    "{}: dma_engine = {:?} is only valid when [device.dma].kind is \"gdma\"",
                    p.name,
                    engine.as_str(),
                );
                let has_any_gdma_host = peri_list.iter().any(|x| {
                    x.dma_user.as_ref().is_some_and(|u| {
                        dma_engine_family_key(&u.engine).ok().as_deref() == Some("gdma")
                    })
                });
                ensure!(
                    has_any_gdma_host,
                    "{}: dma_engine = {:?} but no peripheral has dma_user.engine \"gdma\"",
                    p.name,
                    engine.as_str(),
                );
                parse_gdma_channel_interrupts(p)?;
                continue;
            }

            let _ = dma_engine_family_pascal(&fam_key)?;

            ensure!(
                dma_engine_interrupt_name(p).is_some(),
                "{}: dma_engine requires a PAC interrupt via `interrupts` (e.g. `dma = \"SPI2_DMA\"` or `peri = \"I2S0\"`)",
                p.name
            );

            let hosts = dma_user_hosts_for_pdma_channel(peri_list, p, engine);
            ensure!(
                !hosts.is_empty(),
                "{}: no peripheral has dma_user routing to this PDMA channel (dma_engine {:?}); add dma_user with matching engine id (and `DMA_<Host>` naming or a shared engine such as crypto)",
                p.name,
                engine.as_str(),
            );
        }

        let mut seen_gdma_ch_idx = IndexMap::<u32, String>::new();
        for p in peri_list.iter() {
            let Some(engine) = &p.dma_engine else {
                continue;
            };
            if dma_engine_family_key(engine.as_str()).ok().as_deref() != Some("gdma") {
                continue;
            }
            let idx = gdma_channel_index(&p.name)?;
            ensure!(
                !seen_gdma_ch_idx.contains_key(&idx),
                "duplicate dma_engine \"gdma\" channel index {} ({} conflicts with {:?})",
                idx,
                p.name,
                seen_gdma_ch_idx.get(&idx)
            );
            seen_gdma_ch_idx.insert(idx, p.name.clone());
        }

        Ok(())
    }

    /// The name of the device.
    pub fn name(&self) -> String {
        self.device.name.clone()
    }

    /// The CPU architecture of the device.
    pub fn arch(&self) -> Arch {
        self.device.arch
    }

    /// The core count of the device.
    pub fn cores(&self) -> Cores {
        if self.device.cores > 1 {
            Cores::Multi
        } else {
            Cores::Single
        }
    }

    /// The peripherals of the device.
    pub fn peripherals(&self) -> &[PeripheralDef] {
        self.device
            .peri_config
            .soc
            .as_ref()
            .map(|props| props.config.peripherals.as_slice())
            .unwrap_or(&[])
    }

    /// User-defined symbols for the device.
    pub fn symbols(&self) -> &[String] {
        &self.device.symbols
    }

    /// All configuration values for the device.
    pub fn all(&self) -> &[String] {
        self.all_symbols.get_or_init(|| {
            let mut all = vec![
                self.device.name.clone(),
                self.device.arch.to_string(),
                match self.cores() {
                    Cores::Single => String::from("single_core"),
                    Cores::Multi => String::from("multi_core"),
                },
            ];
            all.extend(self.peripherals().iter().map(|p| p.symbol_name()));
            all.extend_from_slice(&self.device.symbols);
            all.extend(
                self.device
                    .peri_config
                    .driver_names()
                    .map(|name| format!("{name}_driver_supported")),
            );
            all.extend(self.device.peri_config.driver_instances());

            all.extend(
                self.device
                    .peri_config
                    .properties()
                    .filter_map(|(name, optional, value)| {
                        let is_unset = matches!(value, Value::Unset);
                        let mut syms = match value {
                            Value::Boolean(true) => Some(vec![name.to_string()]),
                            Value::NumberList(_) => None,
                            Value::String(value) => Some(vec![format!("{name}=\"{value}\"")]),
                            Value::Generic(v) => v.cfgs(),
                            Value::StringList(values) => Some(
                                values
                                    .iter()
                                    .map(|val| {
                                        format!(
                                            "{name}_{}",
                                            val.to_lowercase().replace("-", "_").replace("/", "_")
                                        )
                                    })
                                    .collect(),
                            ),
                            Value::Number(value) => Some(vec![format!("{name}=\"{value}\"")]),
                            _ => None,
                        };

                        if optional && !is_unset {
                            syms.get_or_insert_default().push(format!("{name}_is_set"));
                        }

                        syms
                    })
                    .flatten(),
            );
            all
        })
    }

    /// Does the configuration contain `item`?
    pub fn contains(&self, item: &str) -> bool {
        self.all().iter().any(|i| i == item)
    }

    pub fn generate_metadata(&self) -> TokenStream {
        let properties = self.generate_properties();
        let peris = self.generate_peripherals();
        let gpios = self.generate_gpios();

        quote! {
            #properties
            #peris
            #gpios
        }
    }

    fn generate_properties(&self) -> TokenStream {
        let chip_name = self.name();
        let chip_pretty_name = Chip::from_str(&chip_name)
            .expect("Valid chip name")
            .pretty_name()
            .to_string();

        // Translate the chip properties into a macro that can be used in esp-hal:
        let arch = self.device.arch.as_ref();
        let cores = number(self.device.cores);
        let trm = &self.device.trm;

        let mut macros = vec![];

        let peripheral_properties =
            self.device
                .peri_config
                .properties()
                .flat_map(|(name, _optional, value)| match value {
                    Value::Number(value) => {
                        let value = number(value); // ensure no numeric suffix is added
                        quote! {
                            (#name) => { #value };
                            (#name, str) => { stringify!(#value) };
                        }
                    }
                    Value::Boolean(value) => quote! {
                        (#name) => { #value };
                    },
                    Value::String(value) => quote! {
                        (#name) => { #value };
                    },
                    Value::NumberList(numbers) => {
                        let numbers = numbers.into_iter().map(number).collect::<Vec<_>>();
                        macros.push(generate_for_each_macro(
                            &name.replace(".", "_"),
                            &[("all", &numbers)],
                        ));
                        quote! {}
                    }
                    Value::Generic(v) => {
                        if let Some(for_each) = v.macros() {
                            macros.push(for_each);
                        }
                        v.property_macro_branches()
                    }
                    Value::Unset | Value::StringList(_) => {
                        quote! {}
                    }
                });

        quote! {
            /// The name of the chip as `&str`
            ///
            /// # Example
            ///
            /// ```rust, no_run
            /// use esp_hal::chip;
            /// let chip_name = chip!();
            #[doc = concat!("assert_eq!(chip_name, ", chip!(), ")")]
            /// ```
            #[macro_export]
            #[cfg_attr(docsrs, doc(cfg(feature = "_device-selected")))]
            macro_rules! chip {
                () => { #chip_name };
            }

            /// The pretty name of the chip as `&str`
            ///
            /// # Example
            ///
            /// ```rust, no_run
            /// use esp_hal::chip;
            /// let chip_name = chip_pretty!();
            #[doc = concat!("assert_eq!(chip_name, ", chip_pretty!(), ")")]
            /// ```
            #[macro_export]
            #[cfg_attr(docsrs, doc(cfg(feature = "_device-selected")))]
            macro_rules! chip_pretty {
                () => { #chip_pretty_name };
            }

            /// The properties of this chip and its drivers.
            #[macro_export]
            #[cfg_attr(docsrs, doc(cfg(feature = "_device-selected")))]
            macro_rules! property {
                ("chip") => { #chip_name };
                ("arch") => { #arch };
                ("cores") => { #cores };
                ("cores", str) => { stringify!(#cores) };
                ("trm") => { #trm };
                #(#peripheral_properties)*
            }

            #(#macros)*
        }
    }

    fn generate_gpios(&self) -> TokenStream {
        let Some(gpio) = self.device.peri_config.gpio.as_ref() else {
            // No GPIOs defined, nothing to do.
            return quote! {};
        };

        cfg::generate_gpios(gpio)
    }

    fn generate_peripherals(&self) -> TokenStream {
        let mut tokens = TokenStream::new();

        // TODO: repeat for all drivers that have Instance traits
        if let Some(peri) = self.device.peri_config.i2c_master.as_ref() {
            tokens.extend(cfg::generate_i2c_master_peripherals(peri));
        };
        if let Some(peri) = self.device.peri_config.uart.as_ref() {
            tokens.extend(cfg::generate_uart_peripherals(peri));
        }
        if let Some(peri) = self.device.peri_config.spi_master.as_ref() {
            tokens.extend(cfg::generate_spi_master_peripherals(peri));
        };
        if let Some(peri) = self.device.peri_config.spi_slave.as_ref() {
            tokens.extend(cfg::generate_spi_slave_peripherals(peri));
        };

        tokens.extend(self.generate_peripherals_macro());

        tokens
    }

    fn generate_peripherals_macro(&self) -> TokenStream {
        let mut all_peripherals = vec![];
        let mut singleton_peripherals = vec![];
        let mut dma_peripherals = vec![];

        let mut stable_peris = vec![];

        for p in self.peripherals().iter() {
            if p.stable && !stable_peris.contains(&p.name.as_str()) {
                stable_peris.push(p.name.as_str());
            }
        }

        if let Some(gpio) = self.device.peri_config.gpio.as_ref() {
            for gpio in gpio.pins_and_signals.pins.iter() {
                let pin = format_ident!("GPIO{}", gpio.pin);
                let mut docs = format!("GPIO{} peripheral singleton", gpio.pin);

                let limitations = gpio.limitations();

                if !limitations.is_empty() {
                    // Append a marker and an explanation to the short description
                    let limitations = limitations
                        .iter()
                        .map(|limitation| format!("<li>{}</li>", limitation))
                        .collect::<Vec<_>>()
                        .join("\n");
                    write!(
                        &mut docs,
                        r#" (Limitations exist)

<section class="warning">
This pin may be available with certain limitations. Check your hardware to make sure whether you can use it.
<ul>
{limitations}
</ul>
</section>"#,
                    )
                    .unwrap();
                }
                let docs = docs.lines();
                let tokens = quote! {
                    #(#[doc = #docs])* #pin <= virtual ()
                };
                all_peripherals.push(quote! { @peri_type #tokens });
                singleton_peripherals.push(quote! { #pin });
            }
        }

        for peri in self.peripherals().iter() {
            let hal = format_ident!("{}", peri.name);
            let pac = if peri.is_virtual {
                format_ident!("virtual")
            } else {
                format_ident!("{}", peri.pac_name.as_deref().unwrap_or(peri.name.as_str()))
            };
            // Make sure we have a stable order
            let mut interrupts = peri.interrupts.iter().collect::<Vec<_>>();
            interrupts.sort_by_key(|(k, _)| k.as_str());
            let interrupts = interrupts.iter().map(|(k, v)| {
                let pac_interrupt_name = format_ident!("{v}");
                let bind = format_ident!("bind_{k}_interrupt");
                let enable = format_ident!("enable_{k}_interrupt");
                let disable = format_ident!("disable_{k}_interrupt");
                quote! { #pac_interrupt_name: { #bind, #enable, #disable } }
            });
            let singleton_doc = format!("{} peripheral singleton", peri.name);
            let tokens = quote! {
                #[doc = #singleton_doc] #hal <= #pac ( #(#interrupts),* )
            };
            if stable_peris
                .iter()
                .any(|p| peri.name.eq_ignore_ascii_case(p))
            {
                all_peripherals.push(quote! { @peri_type #tokens });
                if !peri.hidden {
                    singleton_peripherals.push(quote! { #hal });
                }
            } else {
                all_peripherals.push(quote! { @peri_type #tokens (unstable) });
                if !peri.hidden {
                    singleton_peripherals.push(quote! { #hal (unstable) });
                }
            }

            if let Some(dma_id) = peri.dma_user.as_ref().map(|u| u.peripheral_id) {
                dma_peripherals.push((peri.name.as_str(), dma_id));
            }
        }

        dma_peripherals.sort_by_key(|(_, dma_peripheral)| *dma_peripheral);

        let dma_peripherals = dma_peripherals
            .into_iter()
            .map(|(name, dma_peripheral)| {
                use convert_case::{Boundary, Case, Casing, pattern};

                let peri = format_ident!("{}", name);
                let dma_peripheral = number(dma_peripheral);
                let variant_name = format_ident!(
                    "{}",
                    name.from_case(Case::Custom {
                        boundaries: &[Boundary::LOWER_UPPER, Boundary::UNDERSCORE],
                        pattern: pattern::capital,
                        delim: "",
                    })
                    .to_case(Case::Pascal)
                );
                quote! { #peri, #variant_name, #dma_peripheral }
            })
            .collect::<Vec<_>>();

        let mut pdma_channels = vec![];
        for peri in self.peripherals().iter() {
            let Some(engine) = &peri.dma_engine else {
                continue;
            };
            let fam_key = dma_engine_family_key(engine.as_str())
                .expect("dma_engine should be valid (run Config::validate)");
            if fam_key == "gdma" {
                continue;
            }

            let soc_cfg = format_ident!("{}", peri.symbol_name());
            let instance_ty = format_ident!("{}", peri.name.as_str());

            let Some(interrupt_name) = dma_engine_interrupt_name(peri) else {
                unreachable!("dma_engine rows must pass Config::validate interrupt resolution");
            };
            let interrupt = format_ident!("{}", interrupt_name);
            let pascal = dma_engine_family_pascal(&fam_key)
                .expect("dma_engine should be valid (run Config::validate)");
            let channel_family = format_ident!("{pascal}");
            let regs = format_ident!("{pascal}RegisterBlock");
            let serves = dma_user_hosts_for_pdma_channel(self.peripherals(), peri, engine);
            let pairs = serves.iter().map(|host| {
                let host_ident = format_ident!("{}", host);
                let dma_var = peripheral_dma_variant_ident(host);
                quote! { (#host_ident, #dma_var) }
            });

            pdma_channels.push(quote! {
                #soc_cfg,
                #instance_ty,
                #channel_family,
                #regs,
                #interrupt,
                [#(#pairs),*],
            });
        }

        let mut gdma_channel_rows = vec![];
        for peri in self.peripherals().iter() {
            let Some(engine) = &peri.dma_engine else {
                continue;
            };
            let fam_key = dma_engine_family_key(engine.as_str())
                .expect("dma_engine should be valid (run Config::validate)");
            if fam_key != "gdma" {
                continue;
            }
            let idx = gdma_channel_index(&peri.name).expect("GDMA peripheral names validated");
            let soc_cfg = format_ident!("{}", peri.symbol_name());
            let instance_ty = format_ident!("{}", peri.name.as_str());
            let num_tok = number(idx);
            let irq_row = match parse_gdma_channel_interrupts(peri)
                .expect("GDMA channel interrupts validated")
            {
                GdmaChannelIrqs::Peri(p) => {
                    let irq = format_ident!("{}", p);
                    quote! { #soc_cfg, #instance_ty, #num_tok, #irq }
                }
                GdmaChannelIrqs::RxTx { rx, tx } => {
                    let rx_irq = format_ident!("{}", rx);
                    let tx_irq = format_ident!("{}", tx);
                    quote! { #soc_cfg, #instance_ty, #num_tok, #rx_irq, #tx_irq }
                }
            };
            gdma_channel_rows.push((idx, irq_row));
        }
        gdma_channel_rows.sort_by_key(|(idx, _)| *idx);
        let gdma_channels: Vec<_> = gdma_channel_rows.into_iter().map(|(_, q)| q).collect();

        let peripheral_macros = generate_for_each_macro(
            "peripheral",
            &[
                ("all", &all_peripherals),
                ("singletons", &singleton_peripherals),
                ("dma_eligible", &dma_peripherals),
            ],
        );
        let pdma_macros = if pdma_channels.is_empty() {
            quote! {}
        } else {
            generate_for_each_macro("pdma_channel", &[("all", &pdma_channels)])
        };
        let gdma_macros = if gdma_channels.is_empty() {
            quote! {}
        } else {
            generate_for_each_macro("gdma_channel", &[("all", &gdma_channels)])
        };

        quote! {
            #peripheral_macros
            #pdma_macros
            #gdma_macros
        }
    }

    pub fn active_cfgs(&self) -> Vec<String> {
        let mut cfgs = vec![];

        // Define all necessary configuration symbols for the configured device:
        for symbol in self.all() {
            cfgs.push(symbol.replace('.', "_"));
        }

        cfgs
    }

    /// For each symbol generates a cargo directive that activates it.
    pub fn list_of_cfgs(&self) -> Vec<String> {
        self.active_cfgs()
            .iter()
            .map(|cfg| format!("cargo:rustc-cfg={cfg}"))
            .collect()
    }
}

type Branch<'a> = (&'a str, &'a [TokenStream]);

fn generate_for_each_macro(name: &str, branches: &[Branch<'_>]) -> TokenStream {
    let macro_name = format_ident!("for_each_{name}");

    let flat_branches = branches.iter().flat_map(|b| b.1.iter());
    let repeat_names = branches.iter().map(|b| TokenStream::from_str(b.0).unwrap());
    let repeat_branches = branches.iter().map(|b| b.1);
    let inner = format_ident!("_for_each_inner_{name}");

    quote! {
        // This macro is called in esp-hal to implement a driver's
        // Instance trait for available peripherals. It works by defining, then calling an inner
        // macro that substitutes the properties into the template provided by the call in esp-hal.
        #[macro_export]
        #[cfg_attr(docsrs, doc(cfg(feature = "_device-selected")))]
        macro_rules! #macro_name {
            (
                $($pattern:tt => $code:tt;)*
            ) => {
                macro_rules! #inner {
                    $(($pattern) => $code;)*
                    ($other:tt) => {}
                }

                // Generate single macro calls with each branch
                // Usage:
                // ```
                // for_each_x! {
                //     ( $pattern ) => {
                //         $code
                //     }
                // }
                // ```
                #( #inner!(( #flat_branches ));)*

                // Generate a single macro call with all branches.
                // Usage:
                // ```
                // for_each_x! {
                //     (all $( ($pattern) ),*) => {
                //         $( $code )*
                //     }
                // }
                // ```
                #(  #inner!( (#repeat_names #( (#repeat_branches) ),*) ); )*
            };
        }
    }
}

pub fn generate_build_script_utils() -> TokenStream {
    let check_cfgs = Chip::list_of_check_cfgs();

    let chip = Chip::iter()
        .map(|c| format_ident!("{}", c.name()))
        .collect::<Vec<_>>();
    let feature_env = Chip::iter().map(|c| format!("CARGO_FEATURE_{}", c.as_ref().to_uppercase()));
    let name = Chip::iter()
        .map(|c| c.as_ref().to_string())
        .collect::<Vec<_>>();
    let all_chip_features = name.join(", ");
    let config = Chip::iter().map(|chip| {
        let config = Config::for_chip(&chip);
        let symbols = config.active_cfgs();
        let arch = config.device.arch.to_string();
        let target = config.device.target.as_str();
        let cfgs = config.list_of_cfgs();
        let soc_config = config.device.peri_config.soc.as_ref().unwrap();
        let memory_regions = soc_config.config.memory_map.ranges.iter().map(|r| {
            let name = r.name.as_str();
            let start = number_hex(r.range.start);
            let end = number_hex(r.range.end);
            quote! {
                (#name, MemoryRegion {
                    address_range: #start .. #end,
                })
            }
        });
        let pins = config
            .device
            .peri_config
            .gpio
            .as_ref()
            .map(|gpio| {
                gpio.pins_and_signals
                    .pins
                    .iter()
                    .map(|pin| {
                        let num = number(pin.pin);
                        let limitations = pin.limitations().into_iter().map(|limitation| {
                            TokenStream::from_str(
                                &basic_toml::to_string(&limitation)
                                    .expect("Serializing limitations should be infallible"),
                            )
                            .expect("Valid TOML string can be re-parsed as Rust strings")
                        });
                        quote! {
                            PinInfo {
                                pin: #num,
                                limitations: &[#(#limitations,)*],
                            }
                        }
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        quote! {
            Config {
                architecture: #arch,
                target: #target,
                symbols: &[
                    #(#symbols,)*
                ],
                cfgs: &[
                    #(#cfgs,)*
                ],
                memory_layout: &MemoryLayout {
                    regions: &[
                        #(#memory_regions,)*
                    ],
                },
                pins: &[
                    #(#pins,)*
                ]
            }
        }
    });

    let bail_message = format!(
        "Expected exactly one of the following features to be enabled: {all_chip_features}"
    );

    let from_str_err = format!("Unknown chip {{s}}. Possible options: {all_chip_features}");

    quote! {
        use core::ops::Range;

        extern crate alloc;

        // make it possible to build documentation without `std`.
        #[cfg(docsrs)]
        macro_rules! println {
            ($($any:tt)*) => {};
        }

        #[doc(hidden)]
        #[macro_export]
        macro_rules! __assert_features_logic {
            ($op:tt, $limit:expr, $msg:literal, $($feature:literal),+ $(,)?) => {{
                let enabled: Vec<&str> = [
                    $( if cfg!(feature = $feature) { Some($feature) } else { None }, )+
                ]
                .into_iter()
                .flatten()
                .collect();

                assert!(
                    enabled.len() $op $limit,
                    concat!($msg, ": {}.\nCurrently enabled: {}. This might be caused by enabled default features.\n"),
                    [$($feature),+].join(", "),
                    if enabled.is_empty() {
                        "none".to_string()
                    } else {
                        enabled.join(", ")
                    }
                );
            }};
        }

        #[macro_export]
        macro_rules! assert_unique_features {
            ($($f:literal),+ $(,)?) => {
                $crate::__assert_features_logic!(
                    <=,
                    1,
                    "\nAt most one of the following features must be enabled",
                    $($f),+
                );
            };
        }

        #[macro_export]
        macro_rules! assert_unique_used_features {
            ($($f:literal),+ $(,)?) => {
                $crate::__assert_features_logic!(
                    ==,
                    1,
                    "\nExactly one of the following features must be enabled",
                    $($f),+
                );
            };
        }

        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        #[cfg_attr(docsrs, doc(cfg(feature = "build-script")))]
        pub enum Chip {
            #(#chip),*
        }

        impl core::str::FromStr for Chip {
            type Err = alloc::string::String;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                match s {
                    #( #name => Ok(Self::#chip), )*
                    _ => Err(alloc::format!(#from_str_err)),
                }
            }
        }

        impl Chip {
            /// Tries to extract the active chip from the active cargo features.
            ///
            /// Exactly one device feature must be enabled for this function to succeed.
            pub fn from_cargo_feature() -> Result<Self, &'static str> {
                let all_chips = [
                    #((#feature_env, Self::#chip)),*
                ];

                let mut chip = None;

                for (env, c) in all_chips {
                    if std::env::var(env).is_ok() {
                        if chip.is_some() {
                            return Err(#bail_message);
                        }
                        chip = Some(c);
                    }
                }

                match chip {
                    Some(chip) => Ok(chip),
                    None => Err(#bail_message),
                }
            }

            /// Returns whether the current chip uses the Tensilica Xtensa ISA.
            pub fn is_xtensa(self) -> bool {
                self.config().architecture == "xtensa"
            }

            /// The target triple of the current chip.
            pub fn target(self) -> &'static str {
                self.config().target
            }

            /// The simple name of the current chip.
            ///
            /// ## Example
            ///
            /// ```rust,no_run
            /// assert_eq!(Chip::Esp32s3.name(), "esp32s3");
            /// ```
            pub fn name(self) -> &'static str {
                match self {
                    #( Self::#chip => #name ),*
                }
            }

            /// Returns whether the chip configuration contains the given symbol.
            ///
            /// This function is a short-hand for `self.all_symbols().contains(&symbol)`.
            ///
            /// ## Example
            ///
            /// ```rust,no_run
            /// assert!(Chip::Esp32s3.contains("soc_has_pcnt"));
            /// ```
            pub fn contains(self, symbol: &str) -> bool {
                self.all_symbols().contains(&symbol)
            }

            /// Calling this function will define all cfg symbols for the firmware crate to use.
            pub fn define_cfgs(self) {
                self.config().define_cfgs()
            }

            /// Returns all symbols as a big slice.
            ///
            /// ## Example
            ///
            /// ```rust,no_run
            /// assert!(Chip::Esp32s3.all_symbols().contains("soc_has_pcnt"));
            /// ```
            pub fn all_symbols(&self) -> &'static [&'static str] {
                self.config().symbols
            }

            /// Returns memory layout information.
            pub fn memory_layout(&self) -> &'static MemoryLayout {
                self.config().memory_layout
            }

            /// Returns information about all pins.
            pub fn pins(&self) -> &'static [PinInfo] {
                self.config().pins
            }

            /// Returns an iterator over all chips.
            ///
            /// ## Example
            ///
            /// ```rust,no_run
            /// assert!(Chip::iter().any(|c| c == Chip::Esp32));
            /// ```
            pub fn iter() -> impl Iterator<Item = Chip> {
                [
                    #( Self::#chip ),*
                ]
                .into_iter()
            }

            fn config(self) -> Config {
                match self {
                    #( Self::#chip => #config ),*
                }
            }
        }

        /// Information about a memory region.
        pub struct MemoryRegion {
            address_range: Range<u32>,
        }

        impl MemoryRegion {
            /// Returns the address range of the memory region.
            pub fn range(&self) -> Range<u32> {
                self.address_range.clone()
            }

            /// Returns the size of the memory region in bytes.
            pub fn size(&self) -> u32 {
                self.address_range.end - self.address_range.start
            }
        }

        /// Information about the memory layout of a chip.
        pub struct MemoryLayout {
            regions: &'static [(&'static str, MemoryRegion)],
        }

        impl MemoryLayout {
            /// Returns the memory region with the given name.
            pub fn region(&self, name: &str) -> Option<&'static MemoryRegion> {
                self.regions
                    .iter()
                    .find_map(|(n, r)| if *n == name { Some(r) } else { None })
            }
        }

        /// Information about a specific pin.
        #[non_exhaustive]
        pub struct PinInfo {
            /// The pin number.
            pub pin: usize,

            /// The list of possible restriction categories for this pin.
            ///
            /// This can include "strapping", "spi_psram", etc.
            pub limitations: &'static [&'static str],
        }

        struct Config {
            architecture: &'static str,
            target: &'static str,
            symbols: &'static [&'static str],
            cfgs: &'static [&'static str],
            memory_layout: &'static MemoryLayout,
            pins: &'static [PinInfo],
        }

        impl Config {
            fn define_cfgs(&self) {
                emit_check_cfg_directives();
                for cfg in self.cfgs {
                    println!("{cfg}");
                }
            }
        }

        /// Prints `cargo:rustc-check-cfg` lines.
        pub fn emit_check_cfg_directives() {
            #( println!(#check_cfgs); )*
        }
    }
}

pub fn generate_lib_rs() -> TokenStream {
    let chips = Chip::iter().map(|c| {
        let feature = format!("{c}");
        let file = format!("_generated_{c}.rs");
        quote! {
            #[cfg(feature = #feature)]
            include!(#file);
        }
    });

    quote! {
        //! # (Generated) metadata for Espressif MCUs.
        //!
        //! This crate provides properties that are specific to various Espressif microcontrollers,
        //! and provides macros to work with peripherals, pins, and various other parts of the chips.
        //!
        //! This crate can be used both in firmware, as well as in build scripts, but the usage is different.
        //!
        //! ## Usage in build scripts
        //!
        //! To use the `Chip` enum, add the crate to your `Cargo.toml` build
        //! dependencies, with the `build-script` feature:
        //!
        //! ```toml
        //! [build-dependencies]
        //! esp-metadata-generated = { version = "...", features = ["build-script"] }
        //! ```
        //!
        //! ## Usage in firmware
        //!
        //! To use the various macros, add the crate to your `Cargo.toml` dependencies.
        //! A device-specific feature needs to be enabled in order to use the crate, usually
        //! picked by the user:
        //!
        //! ```toml
        //! [dependencies]
        //! esp-metadata-generated = { version = "..." }
        //! # ...
        //!
        //! [features]
        //! esp32 = ["esp-metadata-generated/esp32"]
        //! esp32c2 = ["esp-metadata-generated/esp32c2"]
        //! # ...
        //! ```
        //!
        //! ## `for_each` macros
        //!
        //! The basic syntax of this macro looks like a macro definition with two distinct syntax options:
        //!
        //! ```rust, no_run
        //! for_each_peripherals! {
        //!     // Individual matcher, invoked separately for each peripheral instance
        //!     ( <individual match syntax> ) => { /* some code */ };
        //!
        //!     // Repeated matcher, invoked once with all peripheral instances
        //!     ( all $( (<individual match syntax>) ),* ) => { /* some code */ };
        //! }
        //! ```
        //!
        //! You can specify any number of matchers in the same invocation.
        //!
        //! > The way code is generated, you will need to use the full `return` syntax to return any
        //! > values from code generated with these macros.
        //!
        //! ### Using the individual matcher
        //!
        //! In this use case, each item's data is individually passed through the macro. This can be used to
        //! generate code for each item separately, allowing specializing the implementation where needed.
        //!
        //! ```rust,no_run
        //! for_each_gpio! {
        //!   // Example data: `(0, GPIO0 (_5 => EMAC_TX_CLK) (_1 => CLK_OUT1 _5 => EMAC_TX_CLK) ([Input] [Output]))`
        //!   ($n:literal, $gpio:ident ($($digital_input_function:ident => $digital_input_signal:ident)*) ($($digital_output_function:ident => $digital_output_signal:ident)*) ($($pin_attribute:ident)*)) => { /* some code */ };
        //!
        //!   // You can create matchers with data filled in. This example will specifically match GPIO2
        //!   ($n:literal, GPIO2 $input_af:tt $output_af:tt $attributes:tt) => { /* Additional case only for GPIO2 */ };
        //! }
        //! ```
        //!
        //! Different macros can have multiple different syntax options for their individual matchers, usually
        //! to provide more detailed information, while preserving simpler syntax for more basic use cases.
        //! Consult each macro's documentation for available options.
        //!
        //! ### Repeated matcher
        //!
        //! With this option, all data is passed through the macro all at once. This form can be used to,
        //! for example, generate struct fields. If the macro has multiple individual matcher options,
        //! there are separate repeated matchers for each of the options.
        //!
        //! To use this option, start the match pattern with the name of the individual matcher option. When
        //! there is only a single individual matcher option, its repeated matcher is named `all` unless
        //! otherwise specified by the macro.
        //!
        //! ```rust,no_run
        //! // Example usage to create a struct containing all GPIOs:
        //! for_each_gpio! {
        //!     (all $( ($n:literal, $gpio:ident $_af_ins:tt $_af_outs:tt $_attrs:tt) ),*) => {
        //!         struct Gpios {
        //!             $(
        //!                 #[doc = concat!(" The ", stringify!($n), "th GPIO pin")]
        //!                 pub $gpio: Gpio<$n>,
        //!             )*
        //!         }
        //!     };
        //! }
        //! ```
        #![cfg_attr(docsrs, feature(doc_cfg))]
        #![cfg_attr(not(feature = "build-script"), no_std)]

        #(#chips)*

        #[cfg(any(feature = "build-script", docsrs))]
        include!( "_build_script_utils.rs");
    }
}

pub fn generate_chip_support_status(output: &mut impl Write) -> std::fmt::Result {
    let nothing = "";

    // Calculate the width of the first column.
    let driver_col_width = std::iter::once("Driver")
        .chain(
            PeriConfig::drivers()
                .iter()
                .filter(|i| !i.hide_from_peri_table)
                .map(|i| i.name),
        )
        .map(|c| c.len())
        .max()
        .unwrap();

    // Header
    write!(output, "| {:driver_col_width$} |", "Driver")?;
    for chip in Chip::iter() {
        write!(output, " {} |", chip.pretty_name())?;
    }
    writeln!(output)?;

    // Header separator
    write!(output, "| {nothing:-<driver_col_width$} |")?;
    for chip in Chip::iter() {
        write!(
            output,
            ":{nothing:-<width$}:|",
            width = chip.pretty_name().len()
        )?;
    }
    writeln!(output)?;

    // Driver support status
    let mut issues = Vec::new();
    for SupportItem {
        name,
        config_group,
        hide_from_peri_table,
    } in PeriConfig::drivers()
    {
        if *hide_from_peri_table {
            continue;
        }
        write!(output, "| {name:driver_col_width$} |")?;
        for chip in Chip::iter() {
            let config = Config::for_chip(&chip);

            let status = config.device.peri_config.support_status(config_group);
            // VSCode displays emojis just a bit wider than 2 characters, making this
            // approximation a bit too wide but good enough.
            let support_cell_width =
                chip.pretty_name().len() - !status.status.icon().is_empty() as usize;
            if let Some(issue) = status.issue {
                write!(output, " [{}][{issue}] [^1] |", status.status.icon())?;
                issues.push(issue);
            } else {
                write!(output, " {:support_cell_width$} |", status.status.icon())?;
            }
        }
        writeln!(output)?;
    }

    writeln!(output)?;
    SupportStatusLevel::write_legend(output)?;
    writeln!(output)?;

    // Print issue link definitions
    issues.sort();
    issues.dedup();

    if !issues.is_empty() {
        writeln!(
            output,
            "[^1]: This cell is clickable and will open the peripheral's issue on GitHub"
        )?;
        writeln!(output)?;
    }
    for issue in issues {
        writeln!(
            output,
            "[{issue}]: https://github.com/esp-rs/esp-hal/issues/{issue}"
        )?;
    }

    Ok(())
}

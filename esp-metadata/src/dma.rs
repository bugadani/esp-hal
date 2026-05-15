//! DMA engine metadata parsing, validation, and macro codegen helpers.

use core::str::FromStr;

use anyhow::{Context, Result, bail, ensure};
use crate::cfg::PeriConfig;
use indexmap::IndexMap;
use quote::{format_ident, quote};

use crate::{PeripheralDef, TokenStream};

/// Host DMA routing: [`DmaUser::engine`] matches a channel peripheral's [`DmaEngine`] string
/// (`gdma`, `spi`, …); `peripheral_id` is the hardware selector.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct DmaUser {
    pub engine: String,
    pub peripheral_id: u32,
}

/// Channel [`DmaEngine`] string: rows share [`for_each_dma_channel!`] with a leading `PDMA` /
/// `GDMA` tag.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(transparent)]
pub struct DmaEngine(String);

impl DmaEngine {
    /// Lowercase engine id (same string as host [`DmaUser::engine`]). Rows feed
    /// [`for_each_dma_channel!`] as `PDMA, …` or `GDMA, …` tuples (family/register block vs channel
    /// index + IRQ group).
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
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

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum DmaChannelMacroSort {
    /// PDMA rows precede GDMA; preserve declaration order among PDMA peripherals.
    Pdma(u32),
    /// GDMA rows ordered by `DMA_CHn` index.
    Gdma(u32),
}

fn quote_dma_channel_row_pdma(
    peri: &PeripheralDef,
    engine: &DmaEngine,
    peripherals: &[PeripheralDef],
) -> TokenStream {
    let instance_ty = format_ident!("{}", peri.name.as_str());
    let interrupt_name =
        dma_engine_interrupt_name(peri).expect("dma_engine rows must include PAC interrupt");
    let interrupt = format_ident!("{}", interrupt_name);
    let fam_key = dma_engine_family_key(engine.as_str())
        .expect("dma_engine should be valid (run Config::validate)");
    let pascal = dma_engine_family_pascal(&fam_key).expect("dma_engine should be valid");
    let channel_family = format_ident!("{pascal}");
    let regs = format_ident!("{pascal}RegisterBlock");
    let serves = dma_user_hosts_for_pdma_channel(peripherals, peri, engine);
    let pairs = serves.iter().map(|host| {
        let host_ident = format_ident!("{}", host);
        let dma_var = peripheral_dma_variant_ident(host);
        quote! { (#host_ident, #dma_var) }
    });
    quote! {
        PDMA,
        #instance_ty,
        #channel_family,
        #regs,
        #interrupt,
        [#(#pairs),*],
    }
}

fn quote_dma_channel_row_gdma(peri: &PeripheralDef) -> TokenStream {
    let idx = gdma_channel_index(&peri.name).expect("GDMA peripheral names validated");
    let instance_ty = format_ident!("{}", peri.name.as_str());
    let num_tok = number(idx);
    match parse_gdma_channel_interrupts(peri).expect("GDMA channel interrupts validated") {
        GdmaChannelIrqs::Peri(p) => {
            let irq = format_ident!("{}", p);
            quote! { GDMA, #instance_ty, #num_tok, (#irq) }
        }
        GdmaChannelIrqs::RxTx { rx, tx } => {
            let rx_irq = format_ident!("{}", rx);
            let tx_irq = format_ident!("{}", tx);
            quote! { GDMA, #instance_ty, #num_tok, (#rx_irq, #tx_irq) }
        }
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

pub(crate) fn peripheral_dma_variant_ident(name: &str) -> proc_macro2::Ident {
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

fn number(n: impl std::fmt::Display) -> TokenStream {
    TokenStream::from_str(&format!("{n}")).unwrap()
}

pub(crate) fn validate(peripherals: &[PeripheralDef], peri_cfg: &PeriConfig) -> Result<()> {
    let peri_list = peripherals;

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
            if chip_has_gdma_controller(peri_cfg) {
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
                chip_has_gdma_controller(peri_cfg),
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

/// Sorted `for_each_dma_channel!` row token streams (PDMA before GDMA, GDMA by channel index).
pub(crate) fn sorted_dma_channel_rows(peripherals: &[PeripheralDef]) -> Vec<TokenStream> {
    let mut dma_channel_entries: Vec<(DmaChannelMacroSort, TokenStream)> = Vec::new();
    let mut pdma_seq = 0u32;
    for peri in peripherals.iter() {
        let Some(engine) = &peri.dma_engine else {
            continue;
        };
        let fam_key = dma_engine_family_key(engine.as_str())
            .expect("dma_engine should be valid (run Config::validate)");
        if fam_key == "gdma" {
            let idx = gdma_channel_index(&peri.name).expect("GDMA peripheral names validated");
            dma_channel_entries.push((
                DmaChannelMacroSort::Gdma(idx),
                quote_dma_channel_row_gdma(peri),
            ));
        } else {
            dma_channel_entries.push((
                DmaChannelMacroSort::Pdma(pdma_seq),
                quote_dma_channel_row_pdma(peri, engine, peripherals),
            ));
            pdma_seq += 1;
        }
    }
    dma_channel_entries.sort_by_key(|(k, _)| *k);
    dma_channel_entries.into_iter().map(|(_, q)| q).collect()
}

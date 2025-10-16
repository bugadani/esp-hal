use std::{collections::HashMap, str::FromStr};

use anyhow::{Context, Result};
use convert_case::{Boundary, Case, Casing, pattern};
use indexmap::IndexMap;
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use serde::{Deserialize, Serialize};

use crate::cfg::{
    Value,
    clock_tree::{ClockTreeItem, ValidationContext},
    soc::clock_tree::PeripheralClockTreeEntry,
};

pub mod clock_tree;

impl super::SocProperties {
    pub(super) fn computed_properties(&self) -> impl Iterator<Item = (&str, bool, Value)> {
        let mut properties = vec![];

        if self.xtal_options.len() > 1 {
            // In this case, the HAL can use `for_each_soc_xtal_options` to see all available
            // options.
            properties.push(("soc.has_multiple_xtal_options", false, Value::Boolean(true)));
        } else {
            properties.push((
                "soc.xtal_frequency",
                false,
                Value::Number(self.xtal_options[0]),
            ));
        }

        properties.into_iter()
    }
}

/// Represents the clock sources and clock distribution tree in the SoC.
#[derive(Debug, Default, Clone, Deserialize)]
pub struct SystemClocks {
    clock_tree: Vec<ClockTreeItem>,
}

struct ProcessedClockData<'c> {
    classified_clocks: IndexMap<&'c str, ClockType<'c>>,
    clock_tree: &'c [ClockTreeItem],
}

impl ProcessedClockData<'_> {
    fn node(&self, name: &str) -> &ClockTreeItem {
        self.clock_tree
            .iter()
            .find(|item| item.name == name)
            .unwrap()
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum ClockType<'s> {
    /// The clock tree item is not configurable.
    Fixed,

    /// The clock tree item is configurable.
    Configurable,

    /// The clock tree item is configured by some other item.
    Dependent(&'s str),
}

impl SystemClocks {
    /// Returns an iterator over the clock tree items.
    fn process(&self) -> Result<ProcessedClockData<'_>> {
        let validation_context = ValidationContext::from(self.clock_tree.as_slice());
        for tree_item in self.clock_tree.iter() {
            tree_item.validate_source_data(&validation_context)?;
        }

        // Classify clock tree items
        // TODO: return clocks in a topological order
        let mut classified_clocks = IndexMap::new();
        for item in self.clock_tree.iter() {
            // If item A configures item B, then item B is a dependent clock tree item.
            item.for_each_configured(|clock| {
                classified_clocks.insert(clock, ClockType::Dependent(&item.name));
            });

            // Each tree item tells its own clock type, except for dependent items. If something has
            // already been classified, we can only change it from its kind to dependent.
            if classified_clocks.get(item.name.as_str()).is_none() {
                classified_clocks.insert(
                    &item.name,
                    if item.is_configurable() {
                        ClockType::Configurable
                    } else {
                        ClockType::Fixed
                    },
                );
            }
        }

        Ok(ProcessedClockData {
            clock_tree: self.clock_tree.as_slice(),
            classified_clocks,
        })
    }

    fn clock_item(&self, name: &str) -> &ClockTreeItem {
        self.clock_tree
            .iter()
            .find(|item| item.name == name)
            .unwrap_or_else(|| panic!("Clock item {} not found", name))
    }
}

impl super::GenericProperty for SystemClocks {
    fn macros(&self) -> Option<proc_macro2::TokenStream> {
        let processed_clocks = self.process().unwrap();

        let mut config_types = vec![];
        let mut configurables = vec![];
        let mut system_config_steps = vec![];
        for (item, kind) in processed_clocks
            .classified_clocks
            .iter()
            .map(|(item, kind)| (item, kind.clone()))
        {
            let clock_item = self.clock_item(item);

            // Definitions every clock node has:
            // - A refcount (technically this could be optimized away in some cases)
            // - request/release functions
            let config_type_decl = clock_item.config_type();
            let definitions = clock_item.clock_node_defs(&processed_clocks);
            let request_release_function_decl =
                clock_item.request_release_functions(&processed_clocks);

            config_types.push(quote! {
                #config_type_decl

                #definitions
                #request_release_function_decl
            });

            if kind == ClockType::Configurable {
                let config_type_name = clock_item.config_type_name();
                let config_apply_function_name = clock_item.config_apply_function_name();

                let item = clock_item.name().to_case(Case::Snake);

                let name = format_ident!("{}", item);

                let docline = clock_item.config_documentation().map(|doc| {
                    // TODO: add explanation what happens if the field is left `None`.
                    let doc = doc.lines();
                    quote! { #(#[doc = #doc])* }
                });

                configurables.push(quote! {
                    #docline
                    #name: Option<#config_type_name>,
                });

                system_config_steps.push(quote! {
                    if let Some(config) = config.#name.as_ref() {
                        // TODO: implement configuration logic
                        // each clock tree node has its own config function, even
                        // if they have the same config type (esp. peripheral clock source nodes)
                        // `configures` options need to generate the right config value,
                        // and pass it to the correct config function
                        #config_apply_function_name(config);
                    }
                });
            }
        }

        // TODO: generate the skeletons of functions that need to be implemented by the HAL, and
        // make it easily copy-pasteable.
        Some(quote! {
            #[macro_export]
            macro_rules! define_clock_tree_types {
                () => {
                    #(#config_types)*

                    /// Clock tree configuration.
                    ///
                    /// The fields of this struct are optional, with the following caveats:
                    // TODO: these should be also generated from metadata, as the list is chip-specific.
                    /// - If `XTL_CLK` is not specified, the crystal frequency will be
                    ///   automatically detected if possible.
                    /// - The CPU and its upstream clock nodes will be set to a default configuration.
                    /// - Other unspecified clock sources will not be useable by peripherals.
                    pub struct ClockConfig {
                        #(#configurables)*
                    }

                    // TODO: temporary name just to get the code's shape
                    fn apply_clock_config(config: &ClockConfig) {
                        #(#system_config_steps)*
                    }

                    /// Simplifies refcounting.
                    trait NodeState {
                        fn refcount(&mut self) -> &mut usize;
                    }

                    /// State type for nodes that only need a reference count.
                    struct RefcountState {
                        refcount: usize,
                    }

                    impl NodeState for RefcountState {
                        fn refcount(&mut self) -> &mut usize {
                            &mut self.refcount
                        }
                    }

                    // TODO: take clock IDs and update bitmaps
                    // static CLOCK_BITMAP: ::portable_atomic::AtomicU32 = ::portable_atomic::AtomicU32::new(0);
                    fn increment_reference_count<S: NodeState>(refcount: &::esp_sync::NonReentrantMutex<S>, callback: impl FnOnce(&mut S)) {
                        refcount.with(|state| {
                            if *state.refcount() == 0 {
                                // CLOCK_BITMAP.fetch_or(!clock_id, Ordering::Relaxed);
                                callback(state);
                            }
                            *state.refcount() += 1;
                        })
                    }
                    fn decrement_reference_count<S: NodeState>(refcount: &::esp_sync::NonReentrantMutex<S>, callback: impl FnOnce(&mut S)) {
                        refcount.with(|state| {
                            *state.refcount() -= 1;
                            if *state.refcount() == 0 {
                                callback(state);
                                // CLOCK_BITMAP.fetch_and(!clock_id, Ordering::Relaxed);
                            }
                        })
                    }
                };
            }
        })
    }
}

/// A named template. Can contain `{{placeholder}}` placeholders that will be substituted with
/// actual values.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct Template {
    /// The name of the template. Other templates can substitute this template's value by using the
    /// `{{name}}` placeholder.
    pub name: String,

    /// The value of the template. Can contain `{{placeholder}}` placeholders that will be
    /// substituted with actual values.
    pub value: String,
}

/// A named peripheral clock signal. These are extracted from the SoC's TRM. Each element generates
/// a `Peripheral::Variant`, and code branches to enable/disable the clock signal, as well as to
/// assert the reset signal of the peripheral.
///
/// `template_params` is a map of substitutions, which will overwrite the defaults set in
/// `PeripheralClocks`. This way each peripheral clock signal can either simply use the defaults,
/// or override them with custom values in case they don't fit the scheme for some reason.
#[derive(Debug, Default, Clone, Deserialize)]
pub struct PeripheralClock {
    /// The name of the peripheral clock signal. Usually specified as CamelCase. Also determines
    /// the value of the `peripheral` template parameter, by converting the name to snake_case.
    pub name: String,

    /// Custom template parameters. These will override the defaults set in `PeripheralClocks`.
    #[serde(default)]
    template_params: HashMap<String, String>,

    /// When true, prevents resetting and disabling the peripheral on startup.
    // TODO: we should do something better, as we keep too many things running. USB/UART depends on
    // esp-println's output option and whether the USB JTAG is connected, TIMG0 is not necessary
    // outside of clock calibrations when the device has Systimer.
    #[serde(default)]
    keep_enabled: bool,

    #[serde(default)]
    #[serde(deserialize_with = "clock_tree::ref_or_def")]
    clocks: PeripheralClockTreeEntry,
}

#[derive(Debug, Default, Clone, Deserialize)]
pub struct PeripheralClocks {
    pub(crate) templates: Vec<Template>,
    pub(crate) peripheral_clocks: Vec<PeripheralClock>,
}

impl PeripheralClocks {
    fn generate_macro(&self) -> Result<TokenStream> {
        let mut clocks = self.peripheral_clocks.clone();
        clocks.sort_by(|a, b| a.name.cmp(&b.name));

        let doclines = clocks.iter().map(|clock| {
            format!(
                "{} peripheral clock signal",
                clock
                    .name
                    .from_case(Case::Custom {
                        boundaries: &[Boundary::LOWER_UPPER, Boundary::DIGIT_UPPER],
                        pattern: pattern::capital,
                        delim: "",
                    })
                    .to_case(Case::UpperSnake)
            )
        });
        let clock_names = clocks
            .iter()
            .map(|clock| quote::format_ident!("{}", clock.name))
            .collect::<Vec<_>>();
        let keep_enabled = clocks.iter().filter_map(|clock| {
            clock
                .keep_enabled
                .then_some(quote::format_ident!("{}", clock.name))
        });

        let clk_en_arms = clocks
            .iter()
            .map(|clock| {
                let clock_name = quote::format_ident!("{}", clock.name);
                let clock_en = self.clk_en(clock)?;

                Ok(quote! {
                    Peripheral::#clock_name => {
                        #clock_en
                    }
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let rst_arms = clocks
            .iter()
            .map(|clock| {
                let clock_name = quote::format_ident!("{}", clock.name);
                let rst = self.rst(clock)?;

                Ok(quote! {
                    Peripheral::#clock_name => {
                        #rst
                    }
                })
            })
            .collect::<Result<Vec<_>>>()?;

        Ok(quote! {
            /// Implement the `Peripheral` enum and enable/disable/reset functions.
            ///
            /// This macro is intended to be placed in `esp_hal::system`.
            #[macro_export]
            #[cfg_attr(docsrs, doc(cfg(feature = "_device-selected")))]
            macro_rules! implement_peripheral_clocks {
                () => {
                    #[doc(hidden)]
                    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
                    #[repr(u8)]
                    #[cfg_attr(feature = "defmt", derive(defmt::Format))]
                    pub enum Peripheral {
                        #(
                            #[doc = #doclines]
                            #clock_names,
                        )*
                    }

                    impl Peripheral {
                        const KEEP_ENABLED: &[Peripheral] = &[
                            #(
                                Self::#keep_enabled,
                            )*
                        ];

                        const COUNT: usize = Self::ALL.len();

                        const ALL: &[Self] = &[
                            #(
                                Self::#clock_names,
                            )*
                        ];
                    }

                    unsafe fn enable_internal_racey(peripheral: Peripheral, enable: bool) {
                        match peripheral {
                            #(#clk_en_arms)*
                        }
                    }

                    unsafe fn assert_peri_reset_racey(peripheral: Peripheral, reset: bool) {
                        match peripheral {
                            #(#rst_arms)*
                        }
                    }
                }
            }
        })
    }

    fn substitute_into(
        &self,
        template_name: &str,
        periph: &PeripheralClock,
    ) -> Result<TokenStream> {
        fn placeholder(name: &str) -> String {
            // format! would work but it needs an insane syntax to escape the curly braces.
            let mut output = String::with_capacity(name.len() + 4);
            output.push_str("{{");
            output.push_str(name);
            output.push_str("}}");
            output
        }

        let mut substitutions = HashMap::new();
        for template in &self.templates {
            substitutions.insert(placeholder(&template.name), template.value.clone());
        }
        substitutions.insert(
            placeholder("peripheral"),
            periph
                .name
                .from_case(Case::Custom {
                    boundaries: &[Boundary::LOWER_UPPER, Boundary::DIGIT_UPPER],
                    pattern: pattern::capital,
                    delim: "",
                })
                .to_case(Case::Snake),
        );
        // Peripheral-specific keys overwrite template defaults
        for (key, value) in periph.template_params.iter() {
            substitutions.insert(placeholder(key), value.clone());
        }

        let template_key = placeholder(template_name);
        let mut template = substitutions[&template_key].clone();

        // Replace while there are substitutions left
        loop {
            let mut found = false;
            for (key, value) in substitutions.iter() {
                if template.contains(key) {
                    template = template.replace(key, value);
                    found = true;
                }
            }
            if !found {
                break;
            }
        }

        match proc_macro2::TokenStream::from_str(&template) {
            Ok(tokens) => Ok(tokens),
            Err(err) => anyhow::bail!("Failed to inflate {template_name}: {err}"),
        }
    }

    fn clk_en(&self, periph: &PeripheralClock) -> Result<TokenStream> {
        self.substitute_into("clk_en_template", periph)
            .with_context(|| format!("Failed to generate clock enable code for {}", periph.name))
    }

    fn rst(&self, periph: &PeripheralClock) -> Result<TokenStream> {
        self.substitute_into("rst_template", periph)
            .with_context(|| format!("Failed to generate reset code for {}", periph.name))
    }
}

impl super::GenericProperty for PeripheralClocks {
    fn macros(&self) -> Option<TokenStream> {
        match self.generate_macro() {
            Ok(tokens) => Some(tokens),
            Err(err) => panic!(
                "{:?}",
                err.context("Failed to generate peripheral clock control macro")
            ),
        }
    }
}

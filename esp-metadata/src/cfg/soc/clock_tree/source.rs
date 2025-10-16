//! Represents the clock tree of an MCU.
//!
//! The clock tree consists of:
//! - Clock sources
//!     - May be configurable or fixed.
//!     - If configurable, the parameter is the desired output frequency.
//! - Clock dividers
//! - Clock muxes
//! - Clock gates
//! - "Derived" clock sources, which act as clock sources derived from other clock sources.
//!
//! Some clock sources are fixed, others are configurable. Some multiplexers and dividers are
//! configured by other elements, others are user-configurable. The output, and some other
//! parameters, of the items is encoded as an expression.
//!
//! Code generation:
//! - An input enum for configurable multiplexers
//!     - we need to deduce which multiplexers are configurable and which are automatically
//!       configured.
//! - `configure` functions with some code filled out
//!     - `configure_impl` function calls for every item, that esp-hal must implement.
//!     - calls to upstream clock tree item `configure` functions
//! - Reference counts
//!     - On clock sources only. `esp_hal::init` is the only place where multiplexers are
//!       configured. Peripheral clock sources can freely be configured by the drivers.
//! - A `clock_source_in_use` bitmap
//!     - This is useful for quickly entering/skipping light sleep in auto-lightsleep mode.
//! - request/release functions that update the reference counts and the bitmap
//! - A cached output frequency value.

use anyhow::Result;
use convert_case::Case;
use proc_macro2::{Ident, TokenStream};
use quote::{format_ident, quote};
use serde::Deserialize;

use crate::cfg::{
    clock_tree::{
        ClockTreeItem,
        Expression,
        ValidationContext,
        ValuesExpression,
        human_readable_frequency,
    },
    soc::ProcessedClockData,
};

#[derive(Debug, Clone, Deserialize)]
pub struct Source {
    /// Output frequency options. If omitted, the source has a fixed frequency.
    #[serde(default)]
    values: Option<ValuesExpression>,

    output: OutputExpression,
}

impl Source {
    pub fn is_configurable(&self) -> bool {
        self.values.is_some() || !self.output.is_constant()
    }

    fn list_of_fixed_frequencies(&self) -> Option<Vec<u32>> {
        self.values.as_ref().and_then(|d| d.as_enum_values())
    }

    pub(super) fn config_docline(&self, clock_name: &str) -> Option<String> {
        if self.values.is_none() {
            return None;
        }

        let docline = if self.list_of_fixed_frequencies().is_some() {
            format!(" Selects the output frequency of `{clock_name}`.")
        } else {
            format!(" The target frequency of the `{clock_name}` clock source.")
        };

        Some(docline)
    }

    pub(super) fn config_type(&self, clock_name: &str, ty_name: &Ident) -> Option<TokenStream> {
        if self.values.is_none() {
            return None;
        }

        if let Some(frequencies) = self.list_of_fixed_frequencies() {
            let mut eval_ctx = somni_expr::Context::new();

            let values = frequencies.iter().map(|freq| format_ident!("_{}", freq));
            let value_doclines = frequencies.iter().map(|freq| {
                eval_ctx.add_variable("VALUE", *freq as u64);
                let output = eval_ctx
                    .evaluate_parsed::<u64>(&self.output.0.source, &self.output.0.expr)
                    .unwrap();

                let (amount, unit) = human_readable_frequency(output);
                format!(" {amount} {unit}")
            });

            Some(quote! {
                #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
                #[cfg_attr(feature = "defmt", derive(defmt::Format))]
                pub enum #ty_name {
                    #(
                        #[doc = #value_doclines]
                        #values,
                    )*
                }
            })
        } else {
            let mut extra_docs = vec![];
            let validate = self.values.as_ref().map(|d| {
                let (min, max) = d.as_range().expect("Invalid frequency range");

                let assert_failed = format!(
                    "`{clock_name}` output frequency value must be between {min} and {max} (inclusive)."
                );

                let (min_readable, min_unit) = human_readable_frequency(min as _);
                let (max_readable, max_unit) = human_readable_frequency(max as _);

                extra_docs = format!(r#"
 # Panics

 Panics if the output frequency value is outside the
 valid range ({min_readable} {min_unit} - {max_readable} {max_unit})."#)
                .lines().map(|l| quote! { #[doc = #l] }).collect();

                quote! { ::core::assert!(frequency >= #min && frequency <= #max, #assert_failed); }
            });

            Some(quote! {
                #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
                #[cfg_attr(feature = "defmt", derive(defmt::Format))]
                pub struct #ty_name(u32);

                impl #ty_name {
                    /// Creates a new clock source configuration.
                    #(#extra_docs)*
                    pub const fn new(frequency: u32) -> Self {
                        #validate
                        Self(frequency)
                    }
                }
            })
        }
    }

    pub(super) fn config_field_docline(&self, clock_name: &str) -> Option<String> {
        Some(format!(" `{clock_name}` configuration."))
    }

    pub(super) fn validate_source_data(&self, _ctx: &ValidationContext<'_>) -> Result<()> {
        Ok(())
    }

    pub(super) fn config_apply_function(
        &self,
        item: &ClockTreeItem,
        tree: &ProcessedClockData<'_>,
    ) -> TokenStream {
        let ty_name = item.config_type_name();
        let apply_fn_name = item.config_apply_function_name();
        let hal_impl = format_ident!("{}_impl", apply_fn_name);
        quote! {
            pub fn #apply_fn_name(config: &#ty_name) {
                #hal_impl(config)
            }

            fn #hal_impl(config: &#ty_name) {
                todo!("This needs to be implemented by the HAL. This is just a placeholder to make the generated code build.")
            }
        }
    }

    pub(crate) fn types(&self) -> TokenStream {
        // Nothing to do here
        quote! {}
    }

    pub(crate) fn node_state(&self) -> (TokenStream, TokenStream) {
        (
            quote! { RefcountState },
            quote! { RefcountState { refcount: 0 } },
        )
    }

    pub(crate) fn request_direct_dependencies(
        &self,
        _this: &ClockTreeItem,
        _tree: &ProcessedClockData<'_>,
    ) -> TokenStream {
        // Normal sources don't have dependencies
        quote! {}
    }

    pub(crate) fn release_direct_dependencies(
        &self,
        _this: &ClockTreeItem,
        _tree: &ProcessedClockData<'_>,
    ) -> TokenStream {
        // Normal sources don't have dependencies
        quote! {}
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct DerivedClockSource {
    #[serde(flatten)]
    source_options: Source,

    from: String,
}

impl DerivedClockSource {
    pub fn upstream_clock(&self) -> Option<&str> {
        Some(&self.from)
    }

    pub fn is_configurable(&self) -> bool {
        self.source_options.is_configurable()
    }

    pub(super) fn config_docline(&self, clock_name: &str) -> Option<String> {
        self.source_options
            .config_docline(clock_name)
            .map(|doc| format!("{} Depends on `{}`.", doc, self.from))
    }

    pub(super) fn config_type(&self, clock_name: &str, ty_name: &Ident) -> Option<TokenStream> {
        self.source_options.config_type(clock_name, ty_name)
    }

    pub(super) fn config_field_docline(&self, clock_name: &str) -> Option<String> {
        self.source_options.config_field_docline(clock_name)
    }

    pub(super) fn validate_source_data(&self, ctx: &ValidationContext<'_>) -> Result<()> {
        anyhow::ensure!(
            ctx.has_clock(&self.from),
            "Clock `{}` is not defined",
            self.from
        );

        self.source_options.validate_source_data(ctx)
    }

    pub(super) fn config_apply_function(
        &self,
        item: &ClockTreeItem,
        tree: &ProcessedClockData<'_>,
    ) -> TokenStream {
        let ty_name = item.config_type_name();
        let apply_fn_name = item.config_apply_function_name();
        quote! {
            pub fn #apply_fn_name(config: &#ty_name) {
                todo!()
            }
        }
    }

    pub(crate) fn types(&self) -> TokenStream {
        // Nothing to do here
        quote! {}
    }

    pub(crate) fn node_state(&self) -> (TokenStream, TokenStream) {
        (
            quote! { RefcountState },
            quote! { RefcountState { refcount: 0 } },
        )
    }

    pub(crate) fn request_direct_dependencies(
        &self,
        this: &ClockTreeItem,
        tree: &ProcessedClockData<'_>,
    ) -> TokenStream {
        let request_fn_name = tree.node(&self.from).request_fn_name();
        quote! {
            #request_fn_name();
        }
    }

    pub(crate) fn release_direct_dependencies(
        &self,
        this: &ClockTreeItem,
        tree: &ProcessedClockData<'_>,
    ) -> TokenStream {
        let release_fn_name = tree.node(&self.from).release_fn_name();
        quote! {
            #release_fn_name();
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct OutputExpression(Expression);

impl OutputExpression {
    fn is_constant(&self) -> bool {
        !self.contains_ident()
    }

    fn contains_ident(&self) -> bool {
        let mut contains = false;
        self.0.visit_variables(|_| contains = true);
        contains
    }
}

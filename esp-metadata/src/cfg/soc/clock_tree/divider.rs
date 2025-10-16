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

use anyhow::{Context, Result};
use proc_macro2::{Ident, TokenStream};
use quote::{format_ident, quote};
use serde::Deserialize;
use somni_parser::{ast, parser::DefaultTypeSet};

use crate::cfg::{
    clock_tree::{ClockTreeItem, Expression, ValidationContext, ValuesExpression},
    soc::ProcessedClockData,
};

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

#[derive(Debug, Clone, Deserialize)]
pub struct Divider {
    /// Possible divider values. May be a list of numbers or a range. If None, the divider value is
    /// fixed.
    #[serde(default)]
    divisors: Option<ValuesExpression>,

    /// The divider equation. The expression contains which clock is being divided. The expression
    /// may refer to clock sources, and the divider's value via `DIVISOR`.
    output: DividerOutputExpression,
}

impl Divider {
    pub fn new(divisors: Option<ValuesExpression>, output: DividerOutputExpression) -> Self {
        Divider { divisors, output }
    }

    pub fn upstream_clock(&self) -> Option<&str> {
        self.find_clock_source()
    }

    pub fn is_configurable(&self) -> bool {
        if self.divisors.is_some() {
            true
        } else {
            let mut contains_divisor = false;
            self.output.0.visit_variables(|var| {
                contains_divisor |= var == "DIVISOR";
            });
            contains_divisor
        }
    }

    pub(super) fn find_clock_source(&self) -> Option<&str> {
        let mut result = None;
        self.output.0.visit_variables(|var| {
            if var != "DIVISOR" {
                if let Some(seen) = result {
                    panic!("A divider cannot combine two clock sources ({seen}, {var})");
                }
                result = Some(var);
            }
        });
        result
    }

    pub(super) fn list_of_fixed_dividers(&self) -> Option<Vec<u32>> {
        self.divisors.as_ref().and_then(|d| d.as_enum_values())
    }

    pub(super) fn config_docline(&self, clock_name: &str) -> Option<String> {
        let expr = &self.output.0.source;
        Some(format!(
            r#" Configures the `{clock_name}` clock divider.

 The output is calculated as `OUTPUT = {expr}`."#
        ))
    }

    pub(super) fn config_type(&self, clock_name: &str, ty_name: &Ident) -> Option<TokenStream> {
        if let Some(dividers) = self.list_of_fixed_dividers() {
            let values = dividers.iter().map(|d| format_ident!("_{}", d));
            let value_doclines = dividers
                .iter()
                .map(|d| format!(" Selects `DIVISOR = {d}`."));

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
            let validate = self.divisors.as_ref().map(|d| {
                let (min, max) = d.as_range().expect("Invalid divisor range");

                let assert_failed = format!(
                    "`{clock_name}` divisor value must be between {min} and {max} (inclusive)."
                );

                extra_docs = format!(
                    r#"
 # Panics

 Panics if the output frequency value is outside the
 valid range ({min} ..= {max})."#
                )
                .lines()
                .map(|l| quote! { #[doc = #l] })
                .collect();

                quote! { ::core::assert!(divisor >= #min && divisor <= #max, #assert_failed); }
            });

            Some(quote! {
                #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
                #[cfg_attr(feature = "defmt", derive(defmt::Format))]
                pub struct #ty_name(u32);

                impl #ty_name {
                    /// Creates a new divider configuration.
                    #(#extra_docs)*
                    pub const fn new(divisor: u32) -> Self {
                        #validate
                        Self(divisor)
                    }
                }
            })
        }
    }

    pub(super) fn config_field_docline(&self, clock_name: &str) -> Option<String> {
        Some(format!(" `{clock_name}` configuration."))
    }

    pub(super) fn validate_source_data(&self, ctx: &ValidationContext<'_>) -> Result<()> {
        let mut result = Ok(());
        self.output.0.visit_variables(|v| {
            if v == "DIVISOR" {
                return;
            }
            if !ctx.has_clock(v) && result.is_ok() {
                result = Err(anyhow::format_err!("{v} is not a valid clock signal name"));
            }
        });
        result
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
        let request_fn_name = tree.node(self.upstream_clock().unwrap()).request_fn_name();
        quote! {
            #request_fn_name();
        }
    }

    pub(crate) fn release_direct_dependencies(
        &self,
        this: &ClockTreeItem,
        tree: &ProcessedClockData<'_>,
    ) -> TokenStream {
        let release_fn_name = tree.node(self.upstream_clock().unwrap()).release_fn_name();
        quote! {
            #release_fn_name();
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct DividerOutputExpression(Expression);

impl DividerOutputExpression {
    fn expr(&self) -> &ast::RightHandExpression<DefaultTypeSet> {
        self.0.expr()
    }
}

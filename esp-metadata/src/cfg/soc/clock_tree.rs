//! Represents the clock tree of an MCU.
//!
//! The clock tree consists of:
//! - Clock sources
//!     - May be configurable or fixed.
//!     - If configurable, the parameter is the desired output frequency.
//! - "Derived" clock sources, which act as clock sources derived from other clock sources.
//! - Clock dividers
//! - Clock muxes
//! - Clock gates
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

use std::str::FromStr;

use anyhow::{Context, Result};
use convert_case::{Boundary, Case, Casing, StateConverter, pattern};
use proc_macro2::{Ident, TokenStream};
use quote::{format_ident, quote};
use serde::{
    Deserialize,
    Deserializer,
    de::{self, SeqAccess, Visitor},
};
use somni_parser::{ast, lexer::Token, parser::DefaultTypeSet};

use crate::cfg::{
    clock_tree::{
        divider::Divider,
        mux::Multiplexer,
        source::{DerivedClockSource, Source},
    },
    soc::ProcessedClockData,
};

mod divider;
mod mux;
mod source;

/// Represents the clock input options for a peripheral.
#[derive(Debug, Clone, Deserialize)]
pub enum PeripheralClockTreeEntry {
    /// Defines clock tree items relevant for the current peripheral.
    Definition(Vec<ClockTreeItem>),

    /// References a clock tree defined in another peripheral. This peripheral will inherit the
    /// clock tree from the referenced peripheral.
    Reference(String),
}

impl Default for PeripheralClockTreeEntry {
    fn default() -> Self {
        PeripheralClockTreeEntry::Definition(Vec::new())
    }
}

// Based on https://serde.rs/string-or-struct.html
pub(super) fn ref_or_def<'de, D>(deserializer: D) -> Result<PeripheralClockTreeEntry, D::Error>
where
    D: Deserializer<'de>,
{
    struct PeripheralClockTreeEntryVisitor;

    impl<'de> Visitor<'de> for PeripheralClockTreeEntryVisitor {
        type Value = PeripheralClockTreeEntry;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("string or list")
        }

        fn visit_str<E>(self, value: &str) -> Result<PeripheralClockTreeEntry, E>
        where
            E: de::Error,
        {
            Ok(PeripheralClockTreeEntry::Reference(value.to_string()))
        }

        fn visit_seq<S>(self, list: S) -> Result<PeripheralClockTreeEntry, S::Error>
        where
            S: SeqAccess<'de>,
        {
            // `SeqAccessDeserializer` is a wrapper that turns a `SeqAccess`
            // into a `Deserializer`, allowing it to be used as the input to T's
            // `Deserialize` implementation. T then deserializes itself using
            // the entries from the map visitor.
            Deserialize::deserialize(de::value::SeqAccessDeserializer::new(list))
                .map(PeripheralClockTreeEntry::Definition)
        }
    }

    deserializer.deserialize_any(PeripheralClockTreeEntryVisitor)
}

pub struct ValidationContext<'c> {
    pub tree: &'c [ClockTreeItem],
}

impl ValidationContext<'_> {
    pub fn has_clock(&self, clk: &str) -> bool {
        self.clock(clk).is_some()
    }

    fn clock(&self, clk: &str) -> Option<&ClockTreeItem> {
        self.tree.iter().find(|item| item.name == clk)
    }
}

impl<'c> From<&'c [ClockTreeItem]> for ValidationContext<'c> {
    fn from(items: &'c [ClockTreeItem]) -> Self {
        ValidationContext { tree: items }
    }
}

/// Represents a clock tree item.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub struct ClockTreeItem {
    /// The unique name of the clock tree item.
    pub name: String,

    /// The type and properties of the clock tree item.
    #[serde(flatten)]
    pub kind: ClockTreeItemKind,

    /// If set, this expression will be used to validate the clock configuration.
    ///
    /// The expression may refer to clock sources, or any of the clock tree item's properties (e.g.
    /// `DIVISOR`).
    #[serde(default)]
    pub reject: Option<RejectExpression>,
}

impl ClockTreeItem {
    pub(crate) fn validate_source_data(&self, ctx: &ValidationContext<'_>) -> Result<()> {
        let result = match &self.kind {
            ClockTreeItemKind::Multiplexer(multiplexer) => multiplexer.validate_source_data(ctx),
            ClockTreeItemKind::Source(source) => source.validate_source_data(ctx),
            ClockTreeItemKind::Divider(divider) => divider.validate_source_data(ctx),
            ClockTreeItemKind::Derived(derived) => derived.validate_source_data(ctx),
        };

        result.with_context(|| format!("Invalid clock tree item: {}", self.name))
    }

    pub(crate) fn is_configurable(&self) -> bool {
        match &self.kind {
            ClockTreeItemKind::Multiplexer(multiplexer) => {
                multiplexer.upstream_clocks().count() > 1
            }
            ClockTreeItemKind::Source(source) => source.is_configurable(),
            ClockTreeItemKind::Divider(divider) => divider.is_configurable(),
            ClockTreeItemKind::Derived(derived) => derived.is_configurable(),
        }
    }

    pub(crate) fn for_each_configured<'s>(&'s self, mut f: impl FnMut(&'s str)) {
        match &self.kind {
            ClockTreeItemKind::Multiplexer(multiplexer) => {
                for clock in multiplexer.configures() {
                    f(clock);
                }
            }
            _ => {}
        }
    }

    /// Returns the documentation for the clock configuration, which will be placed on the
    /// `ClockConfig` field.
    pub(crate) fn config_documentation(&self) -> Option<String> {
        match &self.kind {
            ClockTreeItemKind::Multiplexer(inner) => inner.config_field_docline(&self.name),
            ClockTreeItemKind::Source(inner) => inner.config_field_docline(&self.name),
            ClockTreeItemKind::Divider(inner) => inner.config_field_docline(&self.name),
            ClockTreeItemKind::Derived(inner) => inner.config_field_docline(&self.name),
        }
    }

    pub fn name(&self) -> StateConverter<'_, String> {
        self.name.from_case(Case::Custom {
            boundaries: &[Boundary::LOWER_UPPER, Boundary::UNDERSCORE],
            pattern: pattern::capital,
            delim: "",
        })
    }

    /// Returns the name of the clock configuration type. The corresponding field in the
    /// `ClockConfig` struct will have this type.
    pub(crate) fn config_type_name(&self) -> Option<Ident> {
        if self.is_configurable() {
            let item = self.name().to_case(Case::Pascal);
            Some(quote::format_ident!("{}Config", item))
        } else {
            None
        }
    }

    /// Returns the declaration of the clock configuration type.
    pub(crate) fn config_type(&self) -> Option<TokenStream> {
        let ty_name = self.config_type_name()?;

        let tokens = match &self.kind {
            ClockTreeItemKind::Multiplexer(inner) => inner.config_type(&self.name, &ty_name)?,
            ClockTreeItemKind::Source(inner) => inner.config_type(&self.name, &ty_name)?,
            ClockTreeItemKind::Divider(inner) => inner.config_type(&self.name, &ty_name)?,
            ClockTreeItemKind::Derived(inner) => inner.config_type(&self.name, &ty_name)?,
        };

        let docline = match &self.kind {
            ClockTreeItemKind::Multiplexer(inner) => inner.config_docline(&self.name)?,
            ClockTreeItemKind::Source(inner) => inner.config_docline(&self.name)?,
            ClockTreeItemKind::Divider(inner) => inner.config_docline(&self.name)?,
            ClockTreeItemKind::Derived(inner) => inner.config_docline(&self.name)?,
        };

        let doclines = docline.lines();

        Some(quote! {
            #(#[doc = #doclines])*
            #tokens
        })
    }

    pub(super) fn config_apply_function_name(&self) -> Ident {
        let name = self.name().to_case(Case::Snake);
        format_ident!("apply_{}", name)
    }

    fn request_fn_name(&self) -> Ident {
        let name = self.name().to_case(Case::Snake);
        format_ident!("request_{}", name)
    }

    fn release_fn_name(&self) -> Ident {
        let name = self.name().to_case(Case::Snake);
        format_ident!("release_{}", name)
    }

    fn enable_fn_name(&self) -> Ident {
        let name = self.name().to_case(Case::Snake);
        format_ident!("enable_{}", name)
    }

    fn state_var_name(&self) -> Ident {
        let name = self.name().to_case(Case::UpperSnake);
        format_ident!("{}_STATE", name)
    }

    pub(crate) fn clock_node_defs(&self, tree: &ProcessedClockData<'_>) -> TokenStream {
        let ty_name = self.config_type_name();

        let state_name = self.state_var_name();

        let types = match &self.kind {
            ClockTreeItemKind::Multiplexer(inner) => inner.types(ty_name.as_ref()),
            ClockTreeItemKind::Source(inner) => inner.types(),
            ClockTreeItemKind::Divider(inner) => inner.types(),
            ClockTreeItemKind::Derived(inner) => inner.types(),
        };
        let (state_ty, new_state) = match &self.kind {
            ClockTreeItemKind::Multiplexer(inner) => inner.node_state(ty_name.as_ref()),
            ClockTreeItemKind::Source(inner) => inner.node_state(),
            ClockTreeItemKind::Divider(inner) => inner.node_state(),
            ClockTreeItemKind::Derived(inner) => inner.node_state(),
        };

        // Only configurables have an apply fn
        let apply_fn = ty_name.map(|_| match &self.kind {
            ClockTreeItemKind::Multiplexer(inner) => inner.config_apply_function(self, tree),
            ClockTreeItemKind::Source(inner) => inner.config_apply_function(self, tree),
            ClockTreeItemKind::Divider(inner) => inner.config_apply_function(self, tree),
            ClockTreeItemKind::Derived(inner) => inner.config_apply_function(self, tree),
        });

        quote! {
            #types

            static #state_name: ::esp_sync::NonReentrantMutex<#state_ty> = ::esp_sync::NonReentrantMutex::new(#new_state);

            #apply_fn
        }
    }

    pub(crate) fn request_release_functions(&self, tree: &ProcessedClockData<'_>) -> TokenStream {
        let ty_name = self.config_type_name();

        let request_fn_name = self.request_fn_name();
        let release_fn_name = self.release_fn_name();
        let state_name = self.state_var_name();
        let enable_fn_name = self.enable_fn_name();
        let enable_fn_impl_name = format_ident!("{}_impl", enable_fn_name);

        let (state_ty, _) = match &self.kind {
            ClockTreeItemKind::Multiplexer(inner) => inner.node_state(ty_name.as_ref()),
            ClockTreeItemKind::Source(inner) => inner.node_state(),
            ClockTreeItemKind::Divider(inner) => inner.node_state(),
            ClockTreeItemKind::Derived(inner) => inner.node_state(),
        };

        let request_direct_dependencies = match &self.kind {
            ClockTreeItemKind::Multiplexer(inner) => inner.request_direct_dependencies(self, tree),
            ClockTreeItemKind::Source(inner) => inner.request_direct_dependencies(self, tree),
            ClockTreeItemKind::Divider(inner) => inner.request_direct_dependencies(self, tree),
            ClockTreeItemKind::Derived(inner) => inner.request_direct_dependencies(self, tree),
        };
        let release_direct_dependencies = match &self.kind {
            ClockTreeItemKind::Multiplexer(inner) => inner.release_direct_dependencies(self, tree),
            ClockTreeItemKind::Source(inner) => inner.release_direct_dependencies(self, tree),
            ClockTreeItemKind::Divider(inner) => inner.release_direct_dependencies(self, tree),
            ClockTreeItemKind::Derived(inner) => inner.release_direct_dependencies(self, tree),
        };

        let check_configured = if self.is_configurable() {
            // TODO: If a clock item is requested, configurable, but not configured, fail.
            Some(quote! {
                _ = state;
            })
        } else {
            None
        };
        let user_function_placeholder = {
            let enable_fn_impl_name = enable_fn_impl_name.clone();
            quote! {
                fn #enable_fn_impl_name(_: bool) {
                    todo!("This function needs to be implemented by the HAL. This is just a placeholder to make the generated code build.")
                }
            }
        };
        let (enable_fn_call, disable_fn_call, enable_fn_impl) =
            if let Some(check) = check_configured {
                (
                    quote! { #enable_fn_name(state, true); },
                    quote! { #enable_fn_name(state, false); },
                    quote! {
                        fn #enable_fn_name(state: &mut #state_ty, enable: bool) {
                            #check
                            #enable_fn_impl_name(enable)
                        }

                        #user_function_placeholder
                    },
                )
            } else {
                (
                    quote! { #enable_fn_impl_name(true); },
                    quote! { #enable_fn_impl_name(false); },
                    user_function_placeholder,
                )
            };

        quote! {
            pub fn #request_fn_name() {
                increment_reference_count(&#state_name, |state| {
                    #request_direct_dependencies
                    #enable_fn_call
                });
            }
            pub fn #release_fn_name() {
                decrement_reference_count(&#state_name, |state| {
                    #disable_fn_call
                    #release_direct_dependencies
                });
            }

            #enable_fn_impl
        }
    }
}

/// Represents a clock tree item's type and properties.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub enum ClockTreeItemKind {
    #[serde(rename = "mux")]
    Multiplexer(Multiplexer),

    #[serde(rename = "source")]
    Source(Source),

    #[serde(rename = "divider")]
    Divider(Divider),

    #[serde(rename = "derived")]
    Derived(DerivedClockSource),
}

// Based on https://serde.rs/string-or-struct.html
pub(super) fn list_from_str<'de, D>(deserializer: D) -> Result<Vec<ConfiguresExpression>, D::Error>
where
    D: Deserializer<'de>,
{
    struct ConfigVisitor;

    impl<'de> Visitor<'de> for ConfigVisitor {
        type Value = Vec<ConfiguresExpression>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("list of strings")
        }

        fn visit_seq<S>(self, list: S) -> Result<Vec<ConfiguresExpression>, S::Error>
        where
            S: SeqAccess<'de>,
        {
            // `SeqAccessDeserializer` is a wrapper that turns a `SeqAccess`
            // into a `Deserializer`, allowing it to be used as the input to T's
            // `Deserialize` implementation. T then deserializes itself using
            // the entries from the map visitor.
            Deserialize::deserialize(de::value::SeqAccessDeserializer::new(list))
        }
    }

    deserializer.deserialize_any(ConfigVisitor)
}

/// Two options:
/// - Multiplexer = variant
/// - Divider = value
///
/// TODO: config expressions should be executed before selecting the multiplexer input
#[derive(Debug, Clone)]
pub struct ConfiguresExpression {
    target: String,
    value: Expression,
}

impl ConfiguresExpression {
    fn validate_source_data(&self, ctx: &ValidationContext<'_>) -> Result<()> {
        let Some(clock) = ctx.clock(&self.target) else {
            anyhow::bail!("Clock source {} not found", self.target);
        };

        match &clock.kind {
            ClockTreeItemKind::Multiplexer(multiplexer) => {
                if let Some(name) = self.value.as_name() {
                    if !multiplexer.variant_names().any(|v| v == name) {
                        anyhow::bail!(
                            "Multiplexer `{}` does not have variant `{}`",
                            clock.name,
                            name
                        );
                    }
                } else {
                    anyhow::bail!(
                        "Multiplexer config expression for `{}` must be a name",
                        clock.name
                    );
                }
            }
            ClockTreeItemKind::Divider(divider) => {
                anyhow::ensure!(
                    divider.is_configurable(),
                    "Divider `{}` is not configurable",
                    clock.name,
                )
            }
            _ => anyhow::bail!("Cannot configure source clock {}", self.target),
        }

        Ok(())
    }
}

impl<'de> Deserialize<'de> for ConfiguresExpression {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Ok(Self::from_str(&s).unwrap())
    }
}

impl FromStr for ConfiguresExpression {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (target, value_str) = s.split_once('=').ok_or_else(|| {
            format!("The config expression must be in the format 'target = value'")
        })?;
        let value = somni_parser::parser::parse_expression(value_str)
            .map_err(|e| format!("Failed to parse expression: {}", e))?;
        Ok(ConfiguresExpression {
            target: target.trim().to_string(),
            value: Expression {
                source: value_str.to_string(),
                expr: value,
            },
        })
    }
}

fn human_readable_frequency(mut output: u64) -> (u64, &'static str) {
    let units = ["Hz", "kHz", "MHz", "GHz"];

    let mut index = 0;
    while output >= 1000 && index < units.len() {
        output /= 1000;
        index += 1;
    }

    (output, units[index])
}

#[derive(Debug, Default, Clone)]
pub struct ValuesExpression(Vec<ValueFragment>);

impl ValuesExpression {
    fn as_enum_values(&self) -> Option<Vec<u32>> {
        let frequencies = self
            .0
            .iter()
            .filter_map(|v| match v {
                ValueFragment::FixedFrequency(freq) => Some(*freq),
                _ => None,
            })
            .collect::<Vec<_>>();

        if frequencies.len() == self.0.len() {
            Some(frequencies)
        } else {
            None
        }
    }

    fn as_range(&self) -> Option<(u32, u32)> {
        if self.0.len() != 1 {
            None
        } else {
            let ValueFragment::Range(min, max) = self.0[0] else {
                return None;
            };

            Some((min, max))
        }
    }
}

impl FromStr for ValuesExpression {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let fragments = s
            .split(',')
            .map(|s| s.trim().parse())
            .collect::<Result<Vec<_>, _>>()?;
        Ok(ValuesExpression(fragments))
    }
}

impl<'de> Deserialize<'de> for ValuesExpression {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        FromStr::from_str(&s).map_err(de::Error::custom)
    }
}

#[derive(Debug, Clone, Deserialize)]
pub enum ValueFragment {
    FixedFrequency(u32),
    Range(u32, u32),
}

impl FromStr for ValueFragment {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        fn parse_number(s: &str) -> Result<u32, String> {
            s.replace('_', "")
                .trim()
                .parse()
                .map_err(|e| format!("Invalid number: {}", e))
        }

        if let Some((start, end_incl)) = s.split_once("..=") {
            let start = parse_number(start)?;
            let end = parse_number(end_incl)?;
            Ok(ValueFragment::Range(start, end))
        } else if let Some((start, end_excl)) = s.split_once("..") {
            let start = parse_number(start)?;
            let end = parse_number(end_excl)?;
            Ok(ValueFragment::Range(start, end - 1))
        } else {
            let value = parse_number(s)?;
            Ok(ValueFragment::FixedFrequency(value))
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct RejectExpression(Expression);

#[derive(Debug, Clone)]
struct Expression {
    source: String,
    expr: ast::Expression<DefaultTypeSet>,
}

impl Expression {
    fn lookup(&self, token: Token) -> &str {
        token.source(&self.source)
    }

    fn expr(&self) -> &ast::RightHandExpression<DefaultTypeSet> {
        match &self.expr {
            ast::Expression::Assignment { .. } => unimplemented!("Assignments are not supported"),
            ast::Expression::Expression { expression } => expression,
        }
    }

    fn visit_variables<'s>(&'s self, mut f: impl FnMut(&'s str)) {
        fn visit_variables(
            expr: &ast::RightHandExpression<DefaultTypeSet>,
            f: &mut impl FnMut(Token),
        ) {
            match expr {
                ast::RightHandExpression::Variable { variable } => f(*variable),
                ast::RightHandExpression::Literal { .. } => {}
                ast::RightHandExpression::UnaryOperator { operand, .. } => {
                    visit_variables(operand, f);
                }
                ast::RightHandExpression::BinaryOperator { operands, .. } => {
                    visit_variables(&operands[0], f);
                    visit_variables(&operands[1], f);
                }
                ast::RightHandExpression::FunctionCall { .. } => {
                    panic!("Function calls are not supported")
                }
            }
        }

        visit_variables(self.expr(), &mut |token| f(self.lookup(token)))
    }

    fn as_name(&self) -> Option<&str> {
        if let ast::RightHandExpression::Variable { variable } = self.expr() {
            Some(self.lookup(*variable))
        } else {
            None
        }
    }
}

impl<'de> Deserialize<'de> for Expression {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let expr = somni_parser::parser::parse_expression::<DefaultTypeSet>(&s).unwrap();
        Ok(Expression { source: s, expr })
    }
}

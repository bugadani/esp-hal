//! Clock multiplexer support.

use anyhow::{Context, Result};
use convert_case::{Boundary, Case, Casing, pattern};
use proc_macro2::{Ident, TokenStream};
use quote::{format_ident, quote};
use serde::Deserialize;

use crate::cfg::{
    clock_tree::{ClockTreeItem, ConfiguresExpression, ValidationContext},
    soc::ProcessedClockData,
};

#[derive(Debug, Clone, Deserialize)]
pub struct Multiplexer {
    variants: Vec<MultiplexerVariant>,
}

impl Multiplexer {
    pub fn upstream_clocks(&self) -> impl Iterator<Item = &str> {
        self.variants.iter().map(|v| v.outputs.as_str())
    }

    pub fn variant_names(&self) -> impl Iterator<Item = &str> {
        self.variants.iter().map(|v| v.name.as_str())
    }

    pub fn configures(&self) -> impl Iterator<Item = &str> {
        self.variants
            .iter()
            .flat_map(|v| v.configures.iter().map(|c| c.target.as_str()))
    }

    pub(super) fn config_docline(&self, clock_name: &str) -> Option<String> {
        Some(format!(
            " The list of clock signals that the `{clock_name}` multiplexer can output."
        ))
    }

    pub(super) fn config_field_docline(&self, clock_name: &str) -> Option<String> {
        Some(format!(" `{clock_name}` configuration."))
    }

    pub(super) fn config_type(&self, _clock_name: &str, ty_name: &Ident) -> Option<TokenStream> {
        let variants = self.variants.iter().map(|v| v.config_enum_variant());

        Some(quote! {
            #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
            #[cfg_attr(feature = "defmt", derive(defmt::Format))]
            pub enum #ty_name {
                #(#variants)*
            }
        })
    }

    pub(super) fn validate_source_data(&self, ctx: &ValidationContext<'_>) -> Result<()> {
        for variant in &self.variants {
            variant.validate_source_data(ctx).with_context(|| {
                format!("Multiplexer option {} has incorrect data", variant.name)
            })?;
        }
        Ok(())
    }

    fn state_ty_name(&self, ty_name: &Ident) -> Ident {
        format_ident!("{}State", ty_name)
    }

    pub(super) fn config_apply_function(
        &self,
        item: &ClockTreeItem,
        tree: &ProcessedClockData<'_>,
    ) -> TokenStream {
        let ty_name = item.config_type_name();
        let apply_fn_name = item.config_apply_function_name();
        let hal_impl = format_ident!("{}_impl", apply_fn_name);
        let state = item.state_var_name();

        let request_upstream_fn =
            format_ident!("{}_request_upstream", item.name().to_case(Case::Snake));
        let release_upstream_fn =
            format_ident!("{}_release_upstream", item.name().to_case(Case::Snake));

        let variants = self.variants.iter();

        let request_upstream_branches = variants.clone().map(|variant| {
            let match_arm = variant.config_enum_variant_name();
            let function = tree.node(&variant.outputs).request_fn_name();
            quote! {
                #ty_name::#match_arm => #function()
            }
        });
        let release_upstream_branches = variants.clone().map(|variant| {
            let match_arm = variant.config_enum_variant_name();
            let function = tree.node(&variant.outputs).release_fn_name();
            quote! {
                #ty_name::#match_arm => #function()
            }
        });

        let configures = quote! {
            todo!("Apply `configures` directives");
        };

        quote! {
            pub fn #apply_fn_name(config: &#ty_name) {
                let new_selector = *config;
                #state.with(|state| {
                    let old_selector = state.set_selector(new_selector);

                    #configures

                    if state.refcount > 0 {
                        #request_upstream_fn(new_selector);
                        #hal_impl(old_selector, new_selector);
                        #release_upstream_fn(new_selector);
                    } else {
                        #hal_impl(old_selector, new_selector);
                    }
                });
            }

            pub fn #request_upstream_fn(selector: #ty_name) {
                match selector {
                    #(#request_upstream_branches,)*
                }
            }

            pub fn #release_upstream_fn(selector: #ty_name) {
                match selector {
                    #(#release_upstream_branches,)*
                }
            }

            fn #hal_impl(old_selector: Option<#ty_name>, new_selector: #ty_name) {
                todo!("This needs to be implemented by the HAL. This is just a placeholder to make the generated code build.")
            }
        }
    }

    pub(crate) fn types(&self, config_ty_name: Option<&Ident>) -> TokenStream {
        if let Some(state) = config_ty_name.map(|name| self.state_ty_name(name)) {
            quote! {
                struct #state {
                    refcount: usize,
                    current_selection: Option<#config_ty_name>,
                }

                impl #state {
                    fn set_selector(&mut self, config: #config_ty_name) -> Option<#config_ty_name> {
                        self.current_selection.replace(config)
                    }
                }

                impl NodeState for #state {
                    fn refcount(&mut self) -> &mut usize {
                        &mut self.refcount
                    }
                }
            }
        } else {
            quote! {}
        }
    }

    pub(crate) fn node_state(&self, config_ty_name: Option<&Ident>) -> (TokenStream, TokenStream) {
        if let Some(state) = config_ty_name.map(|name| self.state_ty_name(name)) {
            (
                quote! {
                    #state
                },
                quote! {
                    #state {
                        refcount: 0,
                        current_selection: None,
                    }
                },
            )
        } else {
            (
                quote! { RefcountState },
                quote! { RefcountState { refcount: 0 } },
            )
        }
    }
    pub(crate) fn request_direct_dependencies(
        &self,
        this: &ClockTreeItem,
        tree: &ProcessedClockData<'_>,
    ) -> TokenStream {
        let request_upstream_fn =
            format_ident!("{}_request_upstream", this.name().to_case(Case::Snake));
        quote! {
            if let Some(selector) = state.current_selection {
                #request_upstream_fn(selector);
            }
        }
    }

    pub(crate) fn release_direct_dependencies(
        &self,
        this: &ClockTreeItem,
        tree: &ProcessedClockData<'_>,
    ) -> TokenStream {
        let release_upstream_fn =
            format_ident!("{}_release_upstream", this.name().to_case(Case::Snake));
        quote! {
            if let Some(selector) = state.current_selection {
                #release_upstream_fn(selector);
            }
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct MultiplexerVariant {
    name: String,
    outputs: String,
    #[serde(default, deserialize_with = "super::list_from_str")]
    configures: Vec<ConfiguresExpression>,
}
impl MultiplexerVariant {
    fn config_enum_variant_name(&self) -> Ident {
        format_ident!(
            "{}",
            self.name
                .from_case(Case::Custom {
                    boundaries: &[Boundary::LOWER_UPPER, Boundary::UNDERSCORE],
                    pattern: pattern::capital,
                    delim: "",
                })
                .to_case(Case::Pascal)
        )
    }

    fn config_enum_variant(&self) -> TokenStream {
        let docline = format!(" Selects `{}`.", self.outputs);
        let name = self.config_enum_variant_name();

        quote! {
            #[doc = #docline]
            #name,
        }
    }

    fn validate_source_data(&self, ctx: &ValidationContext<'_>) -> Result<()> {
        anyhow::ensure!(
            ctx.has_clock(&self.outputs),
            "Clock source {} not found",
            self.outputs
        );

        for (index, config) in self.configures.iter().enumerate() {
            config
                .validate_source_data(ctx)
                .with_context(|| format!("Incorrect `configures` expression at index {index}"))?;
        }

        Ok(())
    }
}

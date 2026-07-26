use std::path::PathBuf;

use proc_macro2::TokenStream;
use quote::{format_ident, quote};

use crate::analyzer::Analysis;

mod block;
pub mod common;
mod fields;
mod props;
mod root;
mod state;

pub use root::RootScope;

pub struct Unit {
    pub name: String,
    pub code: TokenStream,
    pub data: Option<Box<[u8]>>,
}

pub struct Scope<'a> {
    pub name: String,
    pub sub_scopes: Box<[Box<dyn ScopeGen<'a> + 'a>]>,
    pub units: Box<[Box<dyn UnitGen + 'a>]>,
}

pub trait ScopeGen<'a> {
    fn generate(&self, analysis: &Analysis) -> Scope<'a>;
}

pub trait UnitGen {
    fn generate(&self, analysis: &Analysis) -> Unit;
}

pub struct Factory<'a> {
    root: PathBuf,
    units: Vec<Box<dyn UnitGen + 'a>>,
    scopes: Vec<Box<dyn ScopeGen<'a> + 'a>>,
}

impl<'a> Factory<'a> {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            units: Vec::new(),
            scopes: Vec::new(),
        }
    }

    #[allow(unused)]
    pub fn add_unit(&mut self, unit: impl UnitGen + 'a) {
        self.units.push(Box::new(unit));
    }

    pub fn add_scope(&mut self, scope: impl ScopeGen<'a> + 'a) {
        self.scopes.push(Box::new(scope));
    }

    fn collect_units(self, analysis: &Analysis) -> Vec<(Vec<String>, Unit)> {
        let mut units = Vec::new();

        for unit in &self.units {
            let unit = unit.generate(analysis);
            units.push((Vec::new(), unit));
        }

        let mut scopes: Vec<(Box<dyn ScopeGen<'a> + 'a>, Vec<String>)> = self
            .scopes
            .into_iter()
            .map(|scope| (scope, Vec::new()))
            .collect();

        while let Some((scope, parent_path)) = scopes.pop() {
            let scope = scope.generate(analysis);
            let scope_name = scope.name.clone();
            let scope_path = if scope_name.is_empty() {
                parent_path
            } else {
                let mut p = parent_path;
                p.push(scope_name);
                p
            };

            for unit in &scope.units {
                let unit = unit.generate(analysis);
                units.push((scope_path.clone(), unit));
            }

            for sub_scope in scope.sub_scopes.into_iter() {
                scopes.push((sub_scope, scope_path.clone()));
            }
        }

        units
    }

    pub fn generate_flat(self, analysis: &Analysis) {
        let output_dir = self.root.clone();
        let units = self.collect_units(analysis);

        let mut bin_entries: Vec<(Vec<String>, String)> = Vec::new();

        for (scope_path, unit) in &units {
            if let Some(data) = &unit.data {
                let bin_name = format!("{}.bin", unit.name);
                let bin_path: PathBuf = scope_path.iter().collect();
                let bin_path = bin_path.join(&bin_name);
                let full_path = output_dir.join(&bin_path);
                std::fs::create_dir_all(full_path.parent().unwrap()).unwrap();
                std::fs::write(&full_path, data).unwrap();
                bin_entries.push((scope_path.clone(), unit.name.clone()));
            }
        }

        let code = units_to_inline_modules(&units);
        let mut code_str = code.to_string();

        for (scope_path, unit_name) in &bin_entries {
            let old = format!("include_bytes ! (\"./{}.bin\")", unit_name);
            let rel_path: String = scope_path
                .iter()
                .chain(std::iter::once(unit_name))
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join("/");
            let new = format!(
                "include_bytes ! (concat!(env!(\"OUT_DIR\"), \"/mcre_world_gen/{}.bin\"))",
                rel_path
            );
            code_str = code_str.replace(&old, &new);
        }

        let file = syn::parse_file(&code_str).unwrap_or_else(|e| {
            panic!("PARSE ERROR: {}", e);
        });
        let source = prettyplease::unparse(&file);

        std::fs::write(output_dir.join("all.rs"), source).unwrap();
    }

    #[allow(dead_code)]
    pub fn generate(self, analysis: &Analysis) {
        let output_dir = self.root.clone();
        let units = self.collect_units(analysis);

        for (scope_path, unit) in &units {
            if let Some(data) = &unit.data {
                let bin_name = format!("{}.bin", unit.name);
                let bin_path: PathBuf = scope_path.iter().collect();
                let bin_path = bin_path.join(&bin_name);
                let full_path = output_dir.join(&bin_path);
                std::fs::create_dir_all(full_path.parent().unwrap()).unwrap();
                std::fs::write(full_path, data).unwrap();
            }
        }

        for (scope_path, unit) in &units {
            let mut dir_path = output_dir.clone();
            for seg in scope_path {
                dir_path.push(seg);
            }
            std::fs::create_dir_all(&dir_path).unwrap();

            let file = syn::parse2(unit.code.clone()).unwrap();
            let source = prettyplease::unparse(&file);
            let file_name = format!("{}.rs", unit.name);
            std::fs::write(dir_path.join(&file_name), source).unwrap();
        }
    }
}

fn units_to_inline_modules(units: &[(Vec<String>, Unit)]) -> TokenStream {
    let mut tree = ModuleNode::new("");

    for (scope_path, unit) in units {
        let is_mod = unit.name == "mod";
        tree.insert(scope_path, is_mod, &unit.name, &unit.code);
    }

    tree.render()
}

struct ModuleNode {
    name: String,
    code: Option<TokenStream>,
    children: Vec<ModuleNode>,
}

impl ModuleNode {
    fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            code: None,
            children: Vec::new(),
        }
    }

    fn insert(&mut self, path: &[String], is_mod: bool, unit_name: &str, code: &TokenStream) {
        if path.is_empty() {
            if is_mod {
                self.code = Some(code.clone());
            } else {
                self.children.push(ModuleNode {
                    name: unit_name.to_string(),
                    code: Some(code.clone()),
                    children: Vec::new(),
                });
            }
            return;
        }

        let segment = &path[0];
        let rest = &path[1..];

        if let Some(child) = self.children.iter_mut().find(|c| c.name == *segment) {
            child.insert(rest, is_mod, unit_name, code);
        } else {
            let mut child = ModuleNode::new(segment);
            child.insert(rest, is_mod, unit_name, code);
            self.children.push(child);
        }
    }

    fn render(&self) -> TokenStream {
        let children: Vec<TokenStream> = self.children.iter().map(|c| c.render()).collect();

        let code = self
            .code
            .as_ref()
            .map(|c| strip_mod_decls(c, &self.children));

        if self.name.is_empty() {
            let code = code.unwrap_or_else(|| quote! {});
            quote! {
                #( #children )*
                #code
            }
        } else {
            let name = format_ident!("{}", &self.name);
            quote! {
                pub mod #name {
                    #code
                    #( #children )*
                }
            }
        }
    }
}

fn strip_mod_decls(code: &TokenStream, children: &[ModuleNode]) -> TokenStream {
    let file: syn::File = syn::parse2(code.clone()).unwrap();
    let child_names: Vec<&str> = children.iter().map(|c| c.name.as_str()).collect();
    let child_names_set: std::collections::HashSet<&str> = child_names.into_iter().collect();

    let items: Vec<TokenStream> = file
        .items
        .into_iter()
        .filter(|item| match item {
            syn::Item::Mod(m) => {
                if m.semi.is_some() && child_names_set.contains(m.ident.to_string().as_str()) {
                    return false;
                }
                true
            }
            _ => true,
        })
        .map(|item| quote! { #item })
        .collect();

    quote! { #( #items )* }
}

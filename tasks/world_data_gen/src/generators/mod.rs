use std::path::{Path, PathBuf};

use proc_macro2::TokenStream;
use tokio::{fs, io};

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

    pub async fn generate(self, analysis: &Analysis<'_>) {
        let mut serialization_units = Vec::new();

        for unit in &self.units {
            let unit = unit.generate(analysis);
            serialization_units.push((None, unit));
        }

        let mut scopes = self
            .scopes
            .into_iter()
            .map(|scope| (scope, PathBuf::new()))
            .collect::<Vec<_>>();

        while let Some((scope, parent_path)) = scopes.pop() {
            let scope = scope.generate(analysis);
            let scope_path = parent_path.join(scope.name);

            for unit in &scope.units {
                let unit = unit.generate(analysis);
                serialization_units.push((Some(scope_path.clone()), unit));
            }

            for sub_scope in scope.sub_scopes {
                scopes.push((sub_scope, scope_path.clone()));
            }
        }

        for (scope, unit) in serialization_units {
            Self::serialize_unit(&self.root, scope, unit).await;
        }
    }

    async fn serialize_unit(root_path: &Path, scope_path: Option<PathBuf>, unit: Unit) {
        let mut path = if let Some(scope_path) = scope_path {
            scope_path.join(unit.name)
        } else {
            PathBuf::from(unit.name)
        };

        path.set_extension("rs");

        let code = unit.code;

        let file = match syn::parse2(code.clone()) {
            Ok(file) => file,
            Err(err) => {
                panic!("Error: {}\n Code: {}", err, code);
            }
        };
        let source = prettyplease::unparse(&file);

        Self::write(root_path, &path, source).await.unwrap();

        if let Some(data) = unit.data {
            path.set_extension("bin");
            Self::write(root_path, path, data).await.unwrap();
        }
    }

    async fn write(
        root_path: &Path,
        path: impl AsRef<Path>,
        content: impl AsRef<[u8]>,
    ) -> io::Result<()> {
        let path = root_path.join(path.as_ref());
        if let Some(parent) = path.parent()
            && !parent.try_exists()?
        {
            fs::create_dir_all(parent).await?;
        }
        fs::write(path, content).await
    }
}

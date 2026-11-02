//! Source-owned declaration and visibility metadata over the independent concrete type catalog.
use super::{ast, modules::Modules, types::ValueType as Ty};
use crate::{Diagnostic, DiagnosticKind as DK, Span, contract::ir as b};
use std::collections::{HashMap, HashSet};
#[derive(Clone)]
pub(crate) struct Declaration {
    pub module: usize,
    pub ast: ast::Data,
    pub id: b::EnumId,
}
pub(crate) struct Catalog {
    pub core: b::Definitions,
    pub declarations: Vec<Declaration>,
    types: Vec<HashMap<String, b::EnumId>>,
    constructors: Vec<HashMap<String, b::CtorRef>>,
}
impl Catalog {
    pub fn collect(modules: &Modules) -> Result<Self, Vec<Diagnostic>> {
        let mut catalog = Self {
            core: b::Definitions::default(),
            declarations: Vec::new(),
            types: vec![HashMap::new(); modules.modules.len()],
            constructors: vec![HashMap::new(); modules.modules.len()],
        };
        let mut errors = Vec::new();
        for (module_index, module) in modules.modules.iter().enumerate() {
            for data in &module.ast.data {
                let mut validate = || -> Result<(), Diagnostic> {
                    if !data
                        .name
                        .value
                        .as_bytes()
                        .first()
                        .is_some_and(u8::is_ascii_uppercase)
                        || !data.name.value.bytes().any(|c| c.is_ascii_lowercase())
                    {
                        return Err(Diagnostic::new(
                            DK::Syntax,
                            data.name.span,
                            "data declarations require a type name",
                        ));
                    }
                    let mut params = HashSet::new();
                    for param in &data.params {
                        if !params.insert(&param.name.value) {
                            return Err(Diagnostic::new(
                                DK::Name,
                                param.name.span,
                                "duplicate type parameter",
                            ));
                        }
                    }
                    let id = catalog.core.enums.alloc(b::EnumDef {
                        name: if module.builtin {
                            data.name.value.clone()
                        } else {
                            format!("{}.{}", module.name, data.name.value)
                        },
                        params: data.params.len(),
                        variants: Vec::new(),
                        error: data.error,
                        external: false,
                    });
                    if catalog.types[module_index]
                        .insert(data.name.value.clone(), id)
                        .is_some()
                    {
                        return Err(Diagnostic::new(
                            DK::Name,
                            data.name.span,
                            "duplicate data type",
                        ));
                    }
                    for (variant, constructor) in data.variants.iter().enumerate() {
                        if catalog.constructors[module_index]
                            .insert(
                                constructor.name.value.clone(),
                                b::CtorRef {
                                    enum_id: id,
                                    variant,
                                },
                            )
                            .is_some()
                        {
                            return Err(Diagnostic::new(
                                DK::Name,
                                constructor.name.span,
                                "duplicate constructor",
                            ));
                        }
                    }
                    catalog.declarations.push(Declaration {
                        module: module_index,
                        ast: data.clone(),
                        id,
                    });
                    Ok(())
                };
                if let Err(error) = validate() {
                    errors.push(error.in_source(&module.source_name));
                }
            }
        }
        if !errors.is_empty() {
            return Err(errors);
        }
        for declaration in catalog.declarations.clone() {
            let params: HashMap<_, _> = declaration
                .ast
                .params
                .iter()
                .enumerate()
                .map(|(i, p)| (p.name.value.clone(), i))
                .collect();
            let result = (|| -> Result<(), Diagnostic> {
                let mut variants = Vec::new();
                for variant in &declaration.ast.variants {
                    let mut names = HashSet::new();
                    let mut tys = Vec::new();
                    for field in &variant.fields {
                        if let Some(name) = &field.name {
                            if !names.insert(name.value.clone()) {
                                return Err(Diagnostic::new(
                                    DK::Name,
                                    name.span,
                                    "duplicate field name",
                                ));
                            }
                        }
                        let ty = catalog.resolve_type(
                            modules,
                            declaration.module,
                            &field.ty,
                            &params,
                        )?;
                        tys.push(backend_type(ty));
                    }
                    let fields = if variant.fields.is_empty() {
                        b::VariantFields::Unit
                    } else if variant.fields[0].name.is_some() {
                        b::VariantFields::Named(
                            variant
                                .fields
                                .iter()
                                .map(|f| f.name.as_ref().unwrap().value.clone())
                                .collect(),
                        )
                    } else {
                        b::VariantFields::Positional(variant.fields.len())
                    };
                    variants.push(b::VariantDef {
                        name: variant.name.value.clone(),
                        fields,
                        tys,
                    });
                }
                catalog.core.enums[declaration.id].variants = variants;
                Ok(())
            })();
            if let Err(error) = result {
                errors.push(error.in_source(&modules.modules[declaration.module].source_name));
            }
        }
        if errors.is_empty() {
            Ok(catalog)
        } else {
            Err(errors)
        }
    }
    pub fn resolve_type(
        &mut self,
        modules: &Modules,
        module: usize,
        text: &ast::Text,
        params: &HashMap<String, usize>,
    ) -> Result<Ty, Diagnostic> {
        self.type_text(modules, module, &text.value, text.span, params, None)
    }
    pub fn resolve_type_with_self(
        &mut self,
        modules: &Modules,
        module: usize,
        text: &ast::Text,
        params: &HashMap<String, usize>,
        self_type: Option<Ty>,
    ) -> Result<Ty, Diagnostic> {
        self.type_text(modules, module, &text.value, text.span, params, self_type)
    }

    fn type_text(
        &mut self,
        modules: &Modules,
        module: usize,
        text: &str,
        span: Span,
        params: &HashMap<String, usize>,
        self_type: Option<Ty>,
    ) -> Result<Ty, Diagnostic> {
        if text == "Self" {
            if let Some(ty) = self_type {
                return Ok(ty);
            }
        }
        if let Some(&parameter) = params.get(text) {
            return Ok(Ty::Compound(
                self.core.types.intern(b::Type::Param(parameter)),
            ));
        }
        let scalar = match text {
            "Int" => Some(Ty::Int),
            "Int64" => Some(Ty::Int64),
            "Float" => Some(Ty::Float),
            "Bool" => Some(Ty::Bool),
            "String" => Some(Ty::String),
            "Unit" => Some(Ty::Unit),
            "Never" => Some(Ty::Never),
            _ => None,
        };
        if let Some(scalar) = scalar {
            return Ok(scalar);
        }
        if text.starts_with('(') && text.ends_with(')') {
            let mut fields = Vec::new();
            for element in split(&text[1..text.len() - 1]) {
                fields.push(backend_type(
                    self.type_text(modules, module, element, span, params, self_type)?,
                ));
            }
            return Ok(Ty::Compound(self.core.types.intern(b::Type::Tuple(fields))));
        }
        let (head, arguments) = if let Some(index) = text.find('[') {
            (&text[..index], split(&text[index + 1..text.len() - 1]))
        } else {
            (text, Vec::new())
        };
        if head == "List" {
            if arguments.len() != 1 {
                return Err(Diagnostic::new(
                    DK::Type,
                    span,
                    "type 'List' expects one argument",
                ));
            }
            let element = backend_type(self.type_text(
                modules,
                module,
                arguments[0],
                span,
                params,
                self_type,
            )?);
            return Ok(Ty::Compound(self.core.types.intern(b::Type::List(element))));
        }
        let id = self
            .lookup_type(modules, module, head)
            .ok_or_else(|| Diagnostic::new(DK::Name, span, format!("undefined type '{head}'")))?;
        let declaration = self.declaration(id);
        if declaration.module != module && !declaration.ast.public {
            return Err(Diagnostic::new(
                DK::Name,
                span,
                format!("type '{head}' is private"),
            ));
        }
        if arguments.len() != declaration.ast.params.len() {
            return Err(Diagnostic::new(
                DK::Type,
                span,
                format!(
                    "type '{head}' expects {} arguments, found {}",
                    declaration.ast.params.len(),
                    arguments.len()
                ),
            ));
        }
        let mut args = Vec::new();
        for argument in arguments {
            args.push(backend_type(
                self.type_text(modules, module, argument, span, params, self_type)?,
            ));
        }
        Ok(Ty::Compound(self.core.types.enum_type(id, args)))
    }
    fn lookup_type(&self, modules: &Modules, module: usize, path: &str) -> Option<b::EnumId> {
        self.lookup(modules, module, path, &self.types, true)
    }
    pub fn constructor(
        &self,
        modules: &Modules,
        module: usize,
        path: &str,
        span: Span,
    ) -> Result<Option<b::CtorRef>, Diagnostic> {
        let Some(ctor) = self.lookup(modules, module, path, &self.constructors, false) else {
            return Ok(None);
        };
        let declaration = self.declaration(ctor.enum_id);
        if declaration.module != module && (!declaration.ast.public || declaration.ast.opaque) {
            return Err(Diagnostic::new(
                DK::Name,
                span,
                format!("constructor '{path}' is private or opaque"),
            ));
        }
        Ok(Some(ctor))
    }
    fn lookup<T: Copy>(
        &self,
        modules: &Modules,
        module: usize,
        path: &str,
        namespace: &[HashMap<String, T>],
        eponymous_type: bool,
    ) -> Option<T> {
        if let Some((alias, name)) = path.split_once('.') {
            let target = modules.modules[module]
                .ast
                .imports
                .iter()
                .find(|i| i.path.value.rsplit('.').next() == Some(alias))
                .and_then(|i| modules.modules.iter().position(|m| m.name == i.path.value))
                .or_else(|| {
                    modules
                        .modules
                        .iter()
                        .position(|m| m.builtin && m.name == alias)
                })?;
            return namespace[target].get(name).copied();
        }
        if let Some(&value) = namespace[module].get(path) {
            return Some(value);
        }
        for import in &modules.modules[module].ast.imports {
            if import
                .members
                .as_ref()
                .is_some_and(|members| members.iter().any(|m| m.value == path))
            {
                let target = modules
                    .modules
                    .iter()
                    .position(|m| m.name == import.path.value)?;
                if let Some(value) = namespace[target].get(path) {
                    return Some(*value);
                }
            }
        }
        modules
            .modules
            .iter()
            .enumerate()
            .filter(|(_, m)| {
                m.builtin && (m.name == "Prelude" || (eponymous_type && m.name == path))
            })
            .find_map(|(index, _)| namespace[index].get(path).copied())
    }
    pub fn type_id(&self, module: usize, name: &str) -> Option<b::EnumId> {
        self.types[module].get(name).copied()
    }
    pub fn declaration(&self, id: b::EnumId) -> &Declaration {
        self.declarations
            .iter()
            .find(|d| d.id == id)
            .expect("source declaration for enum")
    }
    pub fn fields(&mut self, ctor: b::CtorRef, value_type: Ty) -> Vec<Ty> {
        let args = match self.core.ty(backend_type(value_type)) {
            b::Type::Enum(id, args) if *id == ctor.enum_id => args.clone(),
            _ => return Vec::new(),
        };
        self.core.enums[ctor.enum_id].variants[ctor.variant]
            .tys
            .clone()
            .into_iter()
            .map(|ty| frontend_type(self.core.types.subst(ty, &args)))
            .collect()
    }
}
pub(crate) fn backend_type(ty: Ty) -> b::TypeId {
    match ty {
        Ty::Int => b::TypeId::INT,
        Ty::Int64 => b::TypeId::INT64,
        Ty::Float => b::TypeId::FLOAT,
        Ty::Bool => b::TypeId::BOOL,
        Ty::String => b::TypeId::STRING,
        Ty::Unit => b::TypeId::UNIT,
        Ty::Never => b::TypeId::NEVER,
        Ty::Any => b::TypeId::ANY,
        Ty::Compound(id) => id,
    }
}
pub(crate) fn frontend_type(ty: b::TypeId) -> Ty {
    match ty {
        b::TypeId::INT => Ty::Int,
        b::TypeId::INT64 => Ty::Int64,
        b::TypeId::FLOAT => Ty::Float,
        b::TypeId::BOOL => Ty::Bool,
        b::TypeId::STRING => Ty::String,
        b::TypeId::UNIT => Ty::Unit,
        b::TypeId::NEVER => Ty::Never,
        b::TypeId::ANY => Ty::Any,
        _ => Ty::Compound(ty),
    }
}
fn split(text: &str) -> Vec<&str> {
    let mut depth = 0;
    let mut start = 0;
    let mut result = Vec::new();
    for (index, ch) in text.char_indices() {
        match ch {
            '[' | '(' => depth += 1,
            ']' | ')' => depth -= 1,
            ',' if depth == 0 => {
                result.push(&text[start..index]);
                start = index + 1;
            }
            _ => {}
        }
    }
    if start < text.len() {
        result.push(&text[start..]);
    }
    result
}

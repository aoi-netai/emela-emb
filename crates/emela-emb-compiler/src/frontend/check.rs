use super::{
    ast,
    ir::{self, DefinitionId, Local, LocalId},
    modules::Modules,
    types::{InvocationSets, Signature, ValueType as Ty},
};
use crate::{
    Diagnostic, DiagnosticKind as DK, Span,
    sources::{self, Intrinsic},
};
use std::collections::{HashMap, HashSet};

#[derive(Clone)]
struct Definition {
    module: usize,
    ast: ast::Function,
    signature: Signature,
    name: String,
    exported: bool,
    export_symbol: Option<String>,
    external: Option<String>,
    intrinsic: Option<Intrinsic>,
}
#[derive(Clone, Copy)]
enum Symbol {
    Function(DefinitionId),
    Constant(ConstantId),
}
#[derive(Clone, Copy)]
enum ConstantId {
    IntMin,
    IntMax,
    Int64Min,
    Int64Max,
}
impl ConstantId {
    fn ty(self) -> Ty {
        match self {
            Self::IntMin | Self::IntMax => Ty::Int,
            Self::Int64Min | Self::Int64Max => Ty::Int64,
        }
    }
    fn literal(self) -> ir::Literal {
        match self {
            Self::IntMin => ir::Literal::Int(i32::MIN),
            Self::IntMax => ir::Literal::Int(i32::MAX),
            Self::Int64Min => ir::Literal::Int64(i64::MIN),
            Self::Int64Max => ir::Literal::Int64(i64::MAX),
        }
    }
}
#[derive(Default)]
struct Scope {
    values: HashMap<String, Symbol>,
    modules: HashMap<String, usize>,
}
struct Checker {
    modules: Modules,
    definitions: Vec<Definition>,
    module_definitions: Vec<HashMap<String, DefinitionId>>,
    scopes: Vec<Scope>,
    checked: Vec<Option<ir::Function>>,
    states: Vec<u8>,
}
struct Locals {
    names: HashMap<String, Local>,
    next: usize,
}
impl Locals {
    fn bind(&mut self, name: String, ty: Ty) -> Local {
        let local = Local {
            id: LocalId(self.next),
            name: name.clone(),
            ty,
        };
        self.next += 1;
        if name != "_" {
            self.names.insert(name, local.clone());
        }
        local
    }
}

pub(crate) fn check(modules: Modules) -> Result<ir::Program, Vec<Diagnostic>> {
    let mut checker = Checker {
        module_definitions: vec![HashMap::new(); modules.modules.len()],
        scopes: (0..modules.modules.len())
            .map(|_| Scope::default())
            .collect(),
        modules,
        definitions: Vec::new(),
        checked: Vec::new(),
        states: Vec::new(),
    };
    checker.collect()?;
    checker.imports()?;
    checker.checked.resize(checker.definitions.len(), None);
    checker.states.resize(checker.definitions.len(), 0);
    let mut errors = Vec::new();
    for index in 0..checker.definitions.len() {
        if let Err(error) = checker.function(DefinitionId(index)) {
            errors.push(error);
        }
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    Ok(ir::Program {
        functions: checker.checked.into_iter().flatten().collect(),
    })
}
impl Checker {
    fn collect(&mut self) -> Result<(), Vec<Diagnostic>> {
        let mut errors = Vec::new();
        let mut exports = HashSet::new();
        let entry = &self.modules.modules[self.modules.entry];
        if entry.ast.functions.is_empty() && entry.ast.imports.is_empty() {
            return Err(vec![
                Diagnostic::new(
                    DK::Syntax,
                    Span::new(0, 0),
                    "entry source contains no declarations",
                )
                .in_source(&entry.source_name),
            ]);
        }
        for (module_index, module) in self.modules.modules.iter().enumerate() {
            for function in &module.ast.functions {
                let result = (|| -> Result<(), Diagnostic> {
                    if function.name.value == "_" {
                        return Err(Diagnostic::new(
                            DK::Name,
                            function.name.span,
                            "a function cannot be named '_'",
                        ));
                    }
                    let mut names = HashSet::new();
                    let mut params = Vec::new();
                    for param in &function.params {
                        if param.name.value != "_" && !names.insert(param.name.value.clone()) {
                            return Err(Diagnostic::new(
                                DK::Name,
                                param.name.span,
                                format!("duplicate parameter '{}'", param.name.value),
                            ));
                        }
                        params.push(value_type(&param.ty)?);
                    }
                    let ret = function.return_type.as_ref().map(value_type).transpose()?;
                    let mut export_symbol = None;
                    let mut external = None;
                    let mut intrinsic = None;
                    for attribute in &function.attributes {
                        match attribute {
                            ast::Attribute::Export { target, symbol }
                            | ast::Attribute::External { target, symbol } => {
                                if target.value != "c" {
                                    return Err(Diagnostic::new(
                                        DK::Unsupported,
                                        target.span,
                                        "only the C boundary target 'c' is supported",
                                    ));
                                }
                                if !valid_export_name(&symbol.value) {
                                    return Err(Diagnostic::new(
                                        DK::Export,
                                        symbol.span,
                                        "C symbol must be a non-reserved ASCII identifier",
                                    ));
                                }
                                if matches!(attribute, ast::Attribute::Export { .. }) {
                                    if !function.public {
                                        return Err(Diagnostic::new(
                                            DK::Export,
                                            function.name.span,
                                            "@export requires a public function",
                                        ));
                                    }
                                    if export_symbol.replace(symbol.value.clone()).is_some() {
                                        return Err(Diagnostic::new(
                                            DK::Export,
                                            symbol.span,
                                            "duplicate @export attribute",
                                        ));
                                    }
                                    if !exports.insert(symbol.value.clone()) {
                                        return Err(Diagnostic::new(
                                            DK::Export,
                                            symbol.span,
                                            format!("duplicate C export symbol '{}'", symbol.value),
                                        ));
                                    }
                                } else if external.replace(symbol.value.clone()).is_some() {
                                    return Err(Diagnostic::new(
                                        DK::Export,
                                        symbol.span,
                                        "duplicate @external attribute",
                                    ));
                                }
                            }
                            ast::Attribute::Intrinsic(span) => {
                                if !module.builtin {
                                    return Err(Diagnostic::new(
                                        DK::Unsupported,
                                        *span,
                                        "@intrinsic is reserved for compiler-owned declarations",
                                    ));
                                }
                                intrinsic = sources::intrinsic(&module.name, &function.name.value);
                                if intrinsic.is_none() {
                                    return Err(Diagnostic::new(
                                        DK::Unsupported,
                                        *span,
                                        "unregistered core intrinsic declaration",
                                    ));
                                }
                            }
                        }
                    }
                    if external.is_some() || intrinsic.is_some() {
                        if function.body.is_some() {
                            return Err(Diagnostic::new(
                                DK::Type,
                                function.name.span,
                                "external and intrinsic functions cannot have a body",
                            ));
                        }
                        if ret.is_none() {
                            return Err(Diagnostic::new(
                                DK::Type,
                                function.name.span,
                                "external and intrinsic functions require a return type",
                            ));
                        }
                    }
                    let id = DefinitionId(self.definitions.len());
                    if self.module_definitions[module_index]
                        .insert(function.name.value.clone(), id)
                        .is_some()
                    {
                        return Err(Diagnostic::new(
                            DK::Name,
                            function.name.span,
                            format!("duplicate function '{}'", function.name.value),
                        ));
                    }
                    let name = if module_index == self.modules.entry {
                        function.name.value.clone()
                    } else {
                        format!("{}.{}", module.name, function.name.value)
                    };
                    self.definitions.push(Definition {
                        module: module_index,
                        ast: function.clone(),
                        signature: Signature {
                            params,
                            ret,
                            sets: InvocationSets::default(),
                        },
                        name,
                        exported: export_symbol.is_some(),
                        export_symbol,
                        external,
                        intrinsic,
                    });
                    Ok(())
                })();
                if let Err(error) = result {
                    errors.push(error.in_source(&module.source_name));
                }
            }
        }
        let mut external_signatures = HashMap::new();
        for definition in &self.definitions {
            if let Some(symbol) = &definition.external {
                if exports.contains(symbol) {
                    errors.push(self.error(
                        definition.module,
                        DK::Export,
                        definition.ast.name.span,
                        format!("external C symbol '{symbol}' collides with an export"),
                    ));
                }
                let signature = (&definition.signature.params, definition.signature.ret);
                if let Some(previous) = external_signatures.insert(symbol, signature) {
                    if previous != signature {
                        errors.push(self.error(
                            definition.module,
                            DK::Type,
                            definition.ast.name.span,
                            format!("external C symbol '{symbol}' has conflicting signatures"),
                        ));
                    }
                }
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
    fn imports(&mut self) -> Result<(), Vec<Diagnostic>> {
        let module_names: HashMap<_, _> = self
            .modules
            .modules
            .iter()
            .enumerate()
            .map(|(i, m)| (m.name.clone(), i))
            .collect();
        let mut errors = Vec::new();
        for (index, module) in self.modules.modules.iter().enumerate() {
            for import in &module.ast.imports {
                let Some(&target) = module_names.get(&import.path.value) else {
                    continue;
                };
                if let Some(members) = &import.members {
                    for member in members {
                        let symbol =
                            if let Some(id) = self.module_definitions[target].get(&member.value) {
                                if !self.definitions[id.0].ast.public {
                                    errors.push(
                                        Diagnostic::new(
                                            DK::Name,
                                            member.span,
                                            format!(
                                                "'{}.{}' is private",
                                                import.path.value, member.value
                                            ),
                                        )
                                        .in_source(&module.source_name),
                                    );
                                    continue;
                                }
                                Some(Symbol::Function(*id))
                            } else {
                                constant(&import.path.value, &member.value).map(Symbol::Constant)
                            };
                        match symbol {
                            Some(symbol) => {
                                if self.module_definitions[index].contains_key(&member.value)
                                    || self.scopes[index]
                                        .values
                                        .insert(member.value.clone(), symbol)
                                        .is_some()
                                {
                                    errors.push(Diagnostic::new(DK::Name, member.span, format!("imported name '{}' conflicts with another declaration", member.value)).in_source(&module.source_name));
                                }
                            }
                            None => errors.push(
                                Diagnostic::new(
                                    DK::Name,
                                    member.span,
                                    format!(
                                        "module '{}' has no public declaration '{}'",
                                        import.path.value, member.value
                                    ),
                                )
                                .in_source(&module.source_name),
                            ),
                        }
                    }
                } else {
                    let alias = import.path.value.rsplit('.').next().unwrap().to_owned();
                    if self.scopes[index]
                        .modules
                        .insert(alias.clone(), target)
                        .is_some()
                    {
                        errors.push(
                            Diagnostic::new(
                                DK::Name,
                                import.path.span,
                                format!("duplicate imported module '{alias}'"),
                            )
                            .in_source(&module.source_name),
                        );
                    }
                }
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
    fn function(&mut self, id: DefinitionId) -> Result<Ty, Diagnostic> {
        let definition = self.definitions[id.0].clone();
        if self.states[id.0] != 0 {
            return definition.signature.ret.ok_or_else(|| {
                self.error(
                    definition.module,
                    DK::Type,
                    definition.ast.name.span,
                    "recursive functions require an explicit return type",
                )
            });
        }
        self.states[id.0] = 1;
        let mut locals = Locals {
            names: HashMap::new(),
            next: 0,
        };
        let params: Vec<_> = definition
            .ast
            .params
            .iter()
            .zip(&definition.signature.params)
            .map(|(p, ty)| locals.bind(p.name.value.clone(), *ty))
            .collect();
        let body = definition
            .ast
            .body
            .as_ref()
            .map(|body| {
                self.block(
                    definition.module,
                    body,
                    &mut locals,
                    definition.signature.ret,
                )
            })
            .transpose()?;
        let ret = definition
            .signature
            .ret
            .or_else(|| body.as_ref().map(|b| b.ty))
            .unwrap_or(Ty::Unit);
        self.definitions[id.0].signature.ret = Some(ret);
        self.checked[id.0] = Some(ir::Function {
            id,
            name: definition.name,
            source_name: self.modules.modules[definition.module].source_name.clone(),
            span: definition.ast.name.span,
            exported: definition.exported,
            export_symbol: definition.export_symbol,
            signature: Signature {
                ret: Some(ret),
                ..definition.signature
            },
            params,
            external: definition.external,
            intrinsic: definition.intrinsic,
            body,
        });
        self.states[id.0] = 2;
        Ok(ret)
    }
    fn block(
        &mut self,
        module: usize,
        block: &ast::Block,
        locals: &mut Locals,
        expected: Option<Ty>,
    ) -> Result<ir::Block, Diagnostic> {
        let saved = locals.names.clone();
        let mut statements = Vec::new();
        let mut ty = Ty::Unit;
        let mut terminated = false;
        for (index, statement) in block.statements.iter().enumerate() {
            let last = index + 1 == block.statements.len();
            match statement {
                ast::Statement::Bind {
                    name,
                    annotation,
                    value,
                } => {
                    let annotation = annotation
                        .as_ref()
                        .map(value_type)
                        .transpose()
                        .map_err(|e| e.in_source(&self.modules.modules[module].source_name))?;
                    let value = self.expr(module, value, locals, annotation)?;
                    let local = locals.bind(name.value.clone(), value.ty);
                    if value.ty == Ty::Never {
                        terminated = true;
                    }
                    statements.push(ir::Statement::Bind { local, value });
                    ty = Ty::Unit;
                }
                ast::Statement::Expr(expression) => {
                    let expression = self.expr(
                        module,
                        expression,
                        locals,
                        if last { expected } else { None },
                    )?;
                    ty = expression.ty;
                    if ty == Ty::Never {
                        terminated = true;
                    }
                    statements.push(ir::Statement::Expr(expression));
                }
            }
        }
        if terminated {
            ty = Ty::Never;
        }
        if let Some(expected) = expected {
            if !ty.compatible(expected) {
                return Err(self.error(
                    module,
                    DK::Type,
                    block.span,
                    format!("expected {expected}, found {ty}"),
                ));
            }
        }
        locals.names = saved;
        Ok(ir::Block {
            statements,
            ty,
            span: block.span,
        })
    }
    fn expr(
        &mut self,
        module: usize,
        expression: &ast::Expr,
        locals: &mut Locals,
        expected: Option<Ty>,
    ) -> Result<ir::Expr, Diagnostic> {
        use ast::ExprKind as A;
        let span = expression.span;
        let (kind, ty) = match &expression.kind {
            A::Integer(raw) => {
                let target = expected.filter(|t| *t == Ty::Int64).unwrap_or(Ty::Int);
                let clean = raw.replace('_', "");
                let (digits, radix) = if let Some(s) = clean.strip_prefix("0x") {
                    (s, 16)
                } else if let Some(s) = clean.strip_prefix("0b") {
                    (s, 2)
                } else {
                    (clean.as_str(), 10)
                };
                let number = u64::from_str_radix(digits, radix).ok().filter(|n| *n <= if target == Ty::Int64 { i64::MAX as u64 } else { i32::MAX as u64 }).ok_or_else(|| self.error(module, DK::Range, span, format!("integer literal is outside the nonnegative {target} range; use {target}.MIN for the minimum value")))?;
                (
                    ir::ExprKind::Literal(if target == Ty::Int64 {
                        ir::Literal::Int64(number as i64)
                    } else {
                        ir::Literal::Int(number as i32)
                    }),
                    target,
                )
            }
            A::Float(raw) => {
                let number = raw
                    .replace('_', "")
                    .parse::<f64>()
                    .ok()
                    .filter(|n| n.is_finite())
                    .ok_or_else(|| {
                        self.error(
                            module,
                            DK::Range,
                            span,
                            "Float literal is outside the finite binary64 range",
                        )
                    })?;
                (ir::ExprKind::Literal(ir::Literal::Float(number)), Ty::Float)
            }
            A::Bool(value) => (ir::ExprKind::Literal(ir::Literal::Bool(*value)), Ty::Bool),
            A::Unit => (ir::ExprKind::Literal(ir::Literal::Unit), Ty::Unit),
            A::String(value) => (
                ir::ExprKind::Literal(ir::Literal::String(value.clone())),
                Ty::String,
            ),
            A::Interpolate(parts) => {
                let mut checked = Vec::new();
                for part in parts {
                    let value = self.expr(module, part, locals, None)?;
                    if value.ty == Ty::String {
                        checked.push(value);
                    } else {
                        let name = match value.ty {
                            Ty::Int => "Int",
                            Ty::Int64 => "Int64",
                            Ty::Float => "Float",
                            Ty::Bool => "Bool",
                            Ty::Unit => "Unit",
                            Ty::Never => "Unit",
                            Ty::String => unreachable!(),
                        };
                        let target_module = self
                            .modules
                            .modules
                            .iter()
                            .position(|m| m.builtin && m.name == name)
                            .expect("core scalar module");
                        let target = self.module_definitions[target_module]["to_string"];
                        checked.push(ir::Expr {
                            kind: ir::ExprKind::Call {
                                target,
                                args: vec![value],
                                order: vec![0],
                            },
                            ty: Ty::String,
                            span: part.span,
                        });
                    }
                }
                (ir::ExprKind::Concat(checked), Ty::String)
            }
            A::Name(path) => {
                if let Some(local) = (path.len() == 1)
                    .then(|| locals.names.get(&path[0].value))
                    .flatten()
                {
                    (ir::ExprKind::Local(local.id), local.ty)
                } else {
                    match self.resolve(module, path)? {
                        Symbol::Constant(id) => (ir::ExprKind::Literal(id.literal()), id.ty()),
                        Symbol::Function(_) => {
                            return Err(self.error(
                                module,
                                DK::Unsupported,
                                span,
                                "function values require P4; call this function directly",
                            ));
                        }
                    }
                }
            }
            A::Unary { op, value } => {
                let value = self.expr(
                    module,
                    value,
                    locals,
                    if *op == ast::Unary::Not {
                        Some(Ty::Bool)
                    } else {
                        expected.filter(|t| t.numeric())
                    },
                )?;
                if *op == ast::Unary::Neg && !value.ty.numeric() && value.ty != Ty::Never {
                    return Err(self.error(
                        module,
                        DK::Type,
                        span,
                        "unary '-' requires Int, Int64 or Float",
                    ));
                }
                let ty = value.ty;
                (
                    ir::ExprKind::Unary {
                        op: *op,
                        value: Box::new(value),
                    },
                    ty,
                )
            }
            A::Binary {
                op: ast::Binary::Pipe,
                left,
                right,
            } => {
                let (callee, mut args) = match &right.kind {
                    A::Call { callee, args } => (callee.as_ref().clone(), args.clone()),
                    _ => (right.as_ref().clone(), Vec::new()),
                };
                args.insert(
                    0,
                    ast::Argument {
                        label: None,
                        value: left.as_ref().clone(),
                    },
                );
                return self.expr(
                    module,
                    &ast::Expr {
                        kind: A::Call {
                            callee: Box::new(callee),
                            args,
                        },
                        span,
                    },
                    locals,
                    expected,
                );
            }
            A::Binary { op, left, right } => {
                let logical = matches!(op, ast::Binary::And | ast::Binary::Or);
                let hint = if logical {
                    Some(Ty::Bool)
                } else {
                    expected
                        .filter(|t| t.numeric() && !op.comparison())
                        .or_else(|| self.hint(module, left, locals))
                        .or_else(|| self.hint(module, right, locals))
                };
                let left = self.expr(module, left, locals, hint)?;
                let right = self.expr(
                    module,
                    right,
                    locals,
                    if left.ty == Ty::Never {
                        hint
                    } else {
                        Some(left.ty)
                    },
                )?;
                if !logical && !op.comparison() && !left.ty.numeric() && left.ty != Ty::Never {
                    return Err(self.error(
                        module,
                        DK::Type,
                        span,
                        "arithmetic requires Int, Int64 or Float operands",
                    ));
                }
                if op.comparison()
                    && !matches!(op, ast::Binary::Eq | ast::Binary::Ne)
                    && !left.ty.numeric()
                    && !matches!(left.ty, Ty::String | Ty::Bool | Ty::Never)
                {
                    return Err(self.error(
                        module,
                        DK::Type,
                        span,
                        "ordering comparison requires ordered operands",
                    ));
                }
                let ty = if logical || op.comparison() {
                    Ty::Bool
                } else if left.ty == Ty::Never {
                    right.ty
                } else {
                    left.ty
                };
                (
                    ir::ExprKind::Binary {
                        op: *op,
                        left: Box::new(left),
                        right: Box::new(right),
                    },
                    ty,
                )
            }
            A::Call { callee, args } => {
                let A::Name(path) = &callee.kind else {
                    return Err(self.error(
                        module,
                        DK::Unsupported,
                        callee.span,
                        "indirect calls require P4",
                    ));
                };
                if path.len() == 1 && locals.names.contains_key(&path[0].value) {
                    return Err(self.error(
                        module,
                        DK::Type,
                        callee.span,
                        format!(
                            "'{}' is a scalar binding and cannot be called",
                            path[0].value
                        ),
                    ));
                }
                let Symbol::Function(id) = self.resolve(module, path)? else {
                    return Err(self.error(
                        module,
                        DK::Type,
                        callee.span,
                        "a numeric constant cannot be called",
                    ));
                };
                let signature = self.definitions[id.0].signature.clone();
                let params = self.definitions[id.0].ast.params.clone();
                let mut order = vec![usize::MAX; params.len()];
                let mut named = false;
                let mut checked = Vec::new();
                for (index, argument) in args.iter().enumerate() {
                    let parameter = if let Some(label) = &argument.label {
                        named = true;
                        params
                            .iter()
                            .position(|p| p.name.value == label.value)
                            .ok_or_else(|| {
                                self.error(
                                    module,
                                    DK::Type,
                                    label.span,
                                    format!("unknown argument label '{}'", label.value),
                                )
                            })?
                    } else {
                        if named {
                            return Err(self.error(
                                module,
                                DK::Type,
                                argument.value.span,
                                "positional arguments must precede named arguments",
                            ));
                        }
                        if index >= params.len() {
                            return Err(self.error(
                                module,
                                DK::Type,
                                span,
                                format!(
                                    "function '{}' expects {} arguments, found {}",
                                    self.definitions[id.0].name,
                                    params.len(),
                                    args.len()
                                ),
                            ));
                        }
                        index
                    };
                    if order[parameter] != usize::MAX {
                        return Err(self.error(
                            module,
                            DK::Type,
                            argument.value.span,
                            format!(
                                "argument '{}' is supplied more than once",
                                params[parameter].name.value
                            ),
                        ));
                    }
                    order[parameter] = index;
                    checked.push(self.expr(
                        module,
                        &argument.value,
                        locals,
                        Some(signature.params[parameter]),
                    )?);
                }
                if order.contains(&usize::MAX) {
                    return Err(self.error(
                        module,
                        DK::Type,
                        span,
                        format!(
                            "function '{}' expects {} arguments, found {}",
                            self.definitions[id.0].name,
                            params.len(),
                            args.len()
                        ),
                    ));
                }
                let ret = match signature.ret {
                    Some(ret) => ret,
                    None => self.function(id)?,
                };
                (
                    ir::ExprKind::Call {
                        target: id,
                        args: checked,
                        order,
                    },
                    ret,
                )
            }
            A::If {
                condition,
                then,
                else_,
            } => {
                let condition = self.expr(module, condition, locals, Some(Ty::Bool))?;
                let then = self.block(
                    module,
                    then,
                    locals,
                    if else_.is_none() {
                        Some(Ty::Unit)
                    } else {
                        expected
                    },
                )?;
                let else_ = match else_ {
                    Some(block) => self.block(
                        module,
                        block,
                        locals,
                        expected.or_else(|| (then.ty != Ty::Never).then_some(then.ty)),
                    )?,
                    None => ir::Block {
                        statements: Vec::new(),
                        ty: Ty::Unit,
                        span,
                    },
                };
                let ty = if then.ty == Ty::Never {
                    else_.ty
                } else {
                    then.ty
                };
                if !else_.ty.compatible(ty) {
                    return Err(self.error(
                        module,
                        DK::Type,
                        span,
                        format!(
                            "if branches have different types: {} and {}",
                            then.ty, else_.ty
                        ),
                    ));
                }
                (
                    ir::ExprKind::If {
                        condition: Box::new(condition),
                        then,
                        else_,
                    },
                    ty,
                )
            }
            A::Block(block) => {
                let block = self.block(module, block, locals, expected)?;
                let ty = block.ty;
                (ir::ExprKind::Block(block), ty)
            }
            A::Assert(condition) => {
                let condition = self.expr(module, condition, locals, Some(Ty::Bool))?;
                (ir::ExprKind::Assert(Box::new(condition)), Ty::Unit)
            }
        };
        if let Some(expected) = expected {
            if !ty.compatible(expected) {
                return Err(self.error(
                    module,
                    DK::Type,
                    span,
                    format!("expected {expected}, found {ty}"),
                ));
            }
        }
        Ok(ir::Expr { kind, ty, span })
    }
    fn resolve(&self, module: usize, path: &[ast::Text]) -> Result<Symbol, Diagnostic> {
        let first = &path[0];
        if path.len() == 1 {
            if let Some(id) = self.module_definitions[module].get(&first.value) {
                return Ok(Symbol::Function(*id));
            }
            if let Some(symbol) = self.scopes[module].values.get(&first.value) {
                return Ok(*symbol);
            }
        } else if path.len() == 2 {
            let target = self.scopes[module]
                .modules
                .get(&first.value)
                .copied()
                .or_else(|| {
                    self.modules
                        .modules
                        .iter()
                        .position(|m| m.builtin && m.name == first.value)
                });
            if let Some(target) = target {
                if let Some(id) = self.module_definitions[target].get(&path[1].value) {
                    if target != module && !self.definitions[id.0].ast.public {
                        return Err(self.error(
                            module,
                            DK::Name,
                            path[1].span,
                            format!("'{}.{}' is private", first.value, path[1].value),
                        ));
                    }
                    return Ok(Symbol::Function(*id));
                }
                if self.modules.modules[target].builtin {
                    if let Some(id) = constant(&self.modules.modules[target].name, &path[1].value) {
                        return Ok(Symbol::Constant(id));
                    }
                }
            }
        }
        Err(self.error(
            module,
            DK::Name,
            Span::new(first.span.start, path.last().unwrap().span.end),
            format!(
                "undefined name '{}'",
                path.iter()
                    .map(|p| p.value.as_str())
                    .collect::<Vec<_>>()
                    .join(".")
            ),
        ))
    }
    fn hint(&self, module: usize, expression: &ast::Expr, locals: &Locals) -> Option<Ty> {
        match &expression.kind {
            ast::ExprKind::Float(_) => Some(Ty::Float),
            ast::ExprKind::String(_) | ast::ExprKind::Interpolate(_) => Some(Ty::String),
            ast::ExprKind::Bool(_) => Some(Ty::Bool),
            ast::ExprKind::Name(path) => {
                if let Some(local) = (path.len() == 1)
                    .then(|| locals.names.get(&path[0].value))
                    .flatten()
                {
                    return Some(local.ty);
                }
                match self.resolve(module, path).ok()? {
                    Symbol::Constant(id) => Some(id.ty()),
                    Symbol::Function(_) => None,
                }
            }
            ast::ExprKind::Call { callee, .. } => match &callee.kind {
                ast::ExprKind::Name(path) => match self.resolve(module, path).ok()? {
                    Symbol::Function(id) => self.definitions[id.0].signature.ret,
                    Symbol::Constant(_) => None,
                },
                _ => None,
            },
            ast::ExprKind::Unary { value, .. } => self.hint(module, value, locals),
            ast::ExprKind::Binary { op, left, right } if !op.comparison() => self
                .hint(module, left, locals)
                .or_else(|| self.hint(module, right, locals)),
            _ => None,
        }
    }
    fn error(&self, module: usize, kind: DK, span: Span, message: impl Into<String>) -> Diagnostic {
        Diagnostic::new(kind, span, message).in_source(&self.modules.modules[module].source_name)
    }
}
fn constant(module: &str, name: &str) -> Option<ConstantId> {
    match (module, name) {
        ("Int", "MIN") => Some(ConstantId::IntMin),
        ("Int", "MAX") => Some(ConstantId::IntMax),
        ("Int64", "MIN") => Some(ConstantId::Int64Min),
        ("Int64", "MAX") => Some(ConstantId::Int64Max),
        _ => None,
    }
}
fn value_type(text: &ast::Text) -> Result<Ty, Diagnostic> {
    match text.value.as_str() {
        "Int" => Ok(Ty::Int),
        "Int64" => Ok(Ty::Int64),
        "Float" => Ok(Ty::Float),
        "Bool" => Ok(Ty::Bool),
        "String" => Ok(Ty::String),
        "Unit" => Ok(Ty::Unit),
        "Never" => Ok(Ty::Never),
        _ => Err(Diagnostic::new(
            DK::Unsupported,
            text.span,
            format!("type '{}' requires a later phase", text.value),
        )),
    }
}
fn valid_export_name(name: &str) -> bool {
    let mut bytes = name.bytes();
    bytes.next().is_some_and(|b| b.is_ascii_alphabetic())
        && bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_')
        && !matches!(
            name,
            "auto"
                | "break"
                | "case"
                | "char"
                | "const"
                | "continue"
                | "default"
                | "do"
                | "double"
                | "else"
                | "enum"
                | "extern"
                | "float"
                | "for"
                | "goto"
                | "if"
                | "inline"
                | "int"
                | "long"
                | "register"
                | "restrict"
                | "return"
                | "short"
                | "signed"
                | "sizeof"
                | "static"
                | "struct"
                | "switch"
                | "typedef"
                | "union"
                | "unsigned"
                | "void"
                | "volatile"
                | "while"
                | "main"
        )
        && !((name.starts_with("int") || name.starts_with("uint")) && name.ends_with("_t"))
        && ![
            "emela_",
            "EMELA_",
            "INT",
            "UINT",
            "SIZE_",
            "PTRDIFF_",
            "SIG_ATOMIC_",
            "WCHAR_",
            "WINT_",
        ]
        .iter()
        .any(|prefix| name.starts_with(prefix))
}

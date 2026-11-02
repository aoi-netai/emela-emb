//! Propagate row parameter kinds through nominal fields and effect operation signatures.
use super::*;
use crate::frontend::sets::SetKind;

const VALUE: u8 = 1;
const ERROR: u8 = 2;
const EFFECT: u8 = 4;

#[derive(Default)]
pub(super) struct ParameterKinds {
    nominal: HashMap<b::EnumId, Vec<u8>>,
    effects: HashMap<String, Vec<u8>>,
}
fn row_kind(bits: u8) -> Option<SetKind> {
    match bits {
        ERROR => Some(SetKind::Error),
        EFFECT => Some(SetKind::Effect),
        _ => None,
    }
}
impl Catalog {
    pub(super) fn validate_row_argument(
        &self,
        ty: b::TypeId,
        kind: Option<SetKind>,
        span: Span,
    ) -> Result<(), Diagnostic> {
        if self.parameter_kinds.nominal.is_empty() && self.parameter_kinds.effects.is_empty() {
            return Ok(());
        }
        let mut pending = vec![ty];
        let mut seen = HashSet::new();
        while let Some(ty) = pending.pop() {
            if !seen.insert(ty) {
                continue;
            }
            let accepted = match (self.core.ty(ty), kind) {
                (b::Type::Param(_), _) => true,
                (b::Type::Set(row), Some(_)) => {
                    pending.extend(&row.params);
                    true
                }
                (b::Type::Set(_), None) => false,
                (b::Type::Enum(id, _), Some(SetKind::Error)) => self.core.enums[*id].error,
                (b::Type::Handler(_), Some(SetKind::Effect)) => true,
                (_, None) => true,
                _ => false,
            };
            if !accepted {
                return Err(Diagnostic::new(
                    DK::Type,
                    span,
                    match kind {
                        Some(SetKind::Error) => {
                            "an error-set type argument requires errors or an error-set parameter"
                        }
                        Some(SetKind::Effect) => {
                            "an effect-set type argument requires effects or an effect-set parameter"
                        }
                        None => "a value type argument cannot be a set",
                    },
                ));
            }
        }
        Ok(())
    }
    pub fn validate_declared_arguments(&self, modules: &Modules) -> Result<(), Vec<Diagnostic>> {
        let mut errors = Vec::new();
        for declaration in &self.declarations {
            for ty in self.core.enums[declaration.id]
                .variants
                .iter()
                .flat_map(|variant| &variant.tys)
            {
                if let Err(error) = self.validate_nested_arguments(*ty, declaration.ast.name.span) {
                    errors.push(error.in_source(&modules.modules[declaration.module].source_name));
                }
            }
        }
        for declaration in &self.effects.declarations {
            for operation in &self.core.effects[declaration.id].ops {
                for &ty in operation.params.iter().chain([&operation.ret]) {
                    if let Err(error) =
                        self.validate_nested_arguments(ty, declaration.ast.name.span)
                    {
                        errors.push(
                            error.in_source(&modules.modules[declaration.module].source_name),
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
    fn validate_nested_arguments(&self, ty: b::TypeId, span: Span) -> Result<(), Diagnostic> {
        let mut pending = vec![ty];
        let mut seen = HashSet::new();
        while let Some(ty) = pending.pop() {
            if !seen.insert(ty) {
                continue;
            }
            match self.core.ty(ty) {
                b::Type::Enum(id, args) => {
                    for (index, &argument) in args.iter().enumerate() {
                        self.validate_row_argument(
                            argument,
                            self.nominal_parameter_kind(*id, index),
                            span,
                        )?;
                        pending.push(argument);
                    }
                }
                b::Type::Handler(effect) => {
                    for (index, &argument) in self.core.effects[*effect].args.iter().enumerate() {
                        self.validate_row_argument(
                            argument,
                            self.effect_parameter_kind(*effect, index),
                            span,
                        )?;
                        pending.push(argument);
                    }
                }
                b::Type::Fn(signature) => {
                    pending.extend(&signature.params);
                    pending.push(signature.ret);
                    if let Some(sets) = signature.sets {
                        pending.extend([sets.errors, sets.effects]);
                    }
                }
                b::Type::Tuple(fields) => pending.extend(fields),
                b::Type::List(item) | b::Type::Cell(item) => pending.push(*item),
                b::Type::Set(row) => pending.extend(&row.params),
                _ => {}
            }
        }
        Ok(())
    }
    pub fn nominal_parameter_kind(&self, id: b::EnumId, index: usize) -> Option<SetKind> {
        self.parameter_kinds
            .nominal
            .get(&id)?
            .get(index)
            .copied()
            .and_then(row_kind)
    }
    pub fn effect_parameter_kind(&self, id: b::EffectId, index: usize) -> Option<SetKind> {
        self.parameter_kinds
            .effects
            .get(&self.core.effects[id].decl)?
            .get(index)
            .copied()
            .and_then(row_kind)
    }
    pub fn collect_parameter_kinds(&mut self) {
        let mut kinds = ParameterKinds {
            nominal: self
                .core
                .enums
                .iter()
                .map(|(id, definition)| (id, vec![0; definition.params]))
                .collect(),
            effects: self
                .effects
                .declarations
                .iter()
                .map(|declaration| {
                    (
                        self.core.effects[declaration.id].decl.clone(),
                        vec![0; declaration.ast.params.len()],
                    )
                })
                .collect(),
        };
        // First infer all witnessed row positions. Then default phantom parameters
        // to ordinary value types and propagate those defaults through aliases.
        for default_phantoms in [false, true] {
            if default_phantoms {
                for bits in kinds.nominal.values_mut().chain(kinds.effects.values_mut()) {
                    for bit in bits {
                        if *bit == 0 {
                            *bit = VALUE;
                        }
                    }
                }
            }
            loop {
                let mut next = ParameterKinds {
                    nominal: kinds.nominal.clone(),
                    effects: kinds.effects.clone(),
                };
                for (id, definition) in self.core.enums.iter() {
                    let parameters = next.nominal.get_mut(&id).unwrap();
                    for field in definition.variants.iter().flat_map(|variant| &variant.tys) {
                        kinds.record(&self.core, *field, VALUE, parameters);
                    }
                }
                for declaration in &self.effects.declarations {
                    let parameters = next
                        .effects
                        .get_mut(&self.core.effects[declaration.id].decl)
                        .unwrap();
                    for (index, operation) in
                        self.core.effects[declaration.id].ops.iter().enumerate()
                    {
                        for &ty in operation.params.iter().chain([&operation.ret]) {
                            kinds.record(&self.core, ty, VALUE, parameters);
                        }
                        if let Some(errors) = self.effects.operation_errors(declaration.id, index) {
                            for &ty in errors {
                                kinds.record(&self.core, ty, ERROR, parameters);
                            }
                        }
                    }
                }
                if next.nominal == kinds.nominal && next.effects == kinds.effects {
                    break;
                }
                kinds = next;
            }
        }
        self.parameter_kinds = kinds;
    }
}
impl ParameterKinds {
    fn record(&self, core: &b::Definitions, ty: b::TypeId, kind: u8, parameters: &mut [u8]) {
        let mut pending = vec![(ty, kind)];
        let mut seen = HashSet::new();
        while let Some((ty, kind)) = pending.pop() {
            if kind == 0 || !seen.insert((ty, kind)) {
                continue;
            }
            match core.ty(ty) {
                b::Type::Param(index) => {
                    if let Some(bits) = parameters.get_mut(*index) {
                        *bits |= kind;
                    }
                }
                b::Type::Tuple(fields) => pending.extend(fields.iter().map(|&ty| (ty, VALUE))),
                b::Type::List(item) | b::Type::Cell(item) => pending.push((*item, VALUE)),
                b::Type::Enum(id, arguments) => {
                    if let Some(kinds) = self.nominal.get(id) {
                        pending.extend(arguments.iter().zip(kinds).map(|(&ty, &kind)| (ty, kind)));
                    }
                }
                b::Type::Handler(effect) => {
                    let effect = &core.effects[*effect];
                    if let Some(kinds) = self.effects.get(&effect.decl) {
                        pending
                            .extend(effect.args.iter().zip(kinds).map(|(&ty, &kind)| (ty, kind)));
                    }
                }
                b::Type::Fn(signature) => {
                    pending.extend(
                        signature
                            .params
                            .iter()
                            .chain([&signature.ret])
                            .map(|&ty| (ty, VALUE)),
                    );
                    if let Some(sets) = signature.sets {
                        pending.extend([(sets.errors, ERROR), (sets.effects, EFFECT)]);
                    }
                }
                b::Type::Set(row) => pending.extend(row.params.iter().map(|&ty| (ty, kind))),
                _ => {}
            }
        }
    }
}

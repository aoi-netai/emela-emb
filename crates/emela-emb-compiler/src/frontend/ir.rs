//! Source-aware typed IR, before backend normalization. IDs refer to resolved definitions.
use super::{
    ast::{Binary, Unary},
    types::{Signature, ValueType},
};
use crate::{Span, sources::Intrinsic};
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct DefinitionId(pub usize);
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct LocalId(pub usize);
#[derive(Clone, Debug)]
pub(crate) struct Local {
    pub id: LocalId,
    pub name: String,
    pub ty: ValueType,
}
#[derive(Clone, Debug)]
pub(crate) struct Function {
    pub id: DefinitionId,
    pub type_parameters: usize,
    pub constant: bool,
    pub origin: Option<DefinitionId>,
    pub derivation: Option<super::traits::BuiltinTrait>,
    pub name: String,
    pub source_name: String,
    pub span: Span,
    pub exported: bool,
    pub export_symbol: Option<String>,
    pub signature: Signature,
    pub params: Vec<Local>,
    pub external: Option<String>,
    pub intrinsic: Option<Intrinsic>,
    pub body: Option<Block>,
}
#[derive(Clone, Debug)]
pub(crate) struct Program {
    pub core: crate::contract::ir::Definitions,
    pub registry: super::traits::Registry,
    pub functions: Vec<Function>,
}
#[derive(Clone, Debug)]
pub(crate) struct Block {
    pub statements: Vec<Statement>,
    pub ty: ValueType,
    pub span: Span,
}
#[derive(Clone, Debug)]
pub(crate) enum Statement {
    Pattern { pattern: Pattern, value: Expr },
    Expr(Expr),
}
#[derive(Clone, Debug)]
pub(crate) struct Expr {
    pub kind: ExprKind,
    pub ty: ValueType,
    pub span: Span,
}
#[derive(Clone, Debug)]
pub(crate) enum ExprKind {
    Literal(Literal),
    Concat(Vec<Expr>),
    Fail(Box<Expr>),
    Escape {
        value: Box<Expr>,
        error: Local,
        arms: Vec<Arm>,
    },
    Tag(Box<Expr>),
    Builtin {
        op: crate::contract::ir::Builtin,
        args: Vec<Expr>,
    },
    TraitCall {
        trait_id: super::traits::TraitId,
        method: String,
        self_type: crate::contract::ir::TypeId,
        args: Vec<Expr>,
        order: Vec<usize>,
        nested_show: bool,
    },
    Match {
        value: Box<Expr>,
        arms: Vec<Arm>,
    },
    Tuple(Vec<Expr>),
    Make {
        ctor: crate::contract::ir::CtorRef,
        args: Vec<Expr>,
        order: Vec<usize>,
    },
    Field {
        base: Box<Expr>,
        ctor: crate::contract::ir::CtorRef,
        index: usize,
    },
    Local(LocalId),
    Unary {
        op: Unary,
        value: Box<Expr>,
    },
    Binary {
        op: Binary,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    Call {
        target: DefinitionId,
        type_arguments: Vec<crate::contract::ir::TypeId>,
        args: Vec<Expr>,
        order: Vec<usize>,
    },
    If {
        condition: Box<Expr>,
        then: Block,
        else_: Block,
    },
    Block(Block),
    Assert(Box<Expr>),
}
#[derive(Clone, Debug)]
pub(crate) enum Literal {
    Int(i32),
    Int64(i64),
    Float(f64),
    String(String),
    Bool(bool),
    Unit,
}

#[derive(Clone, Debug)]
pub(crate) struct Pattern {
    pub kind: PatternKind,
    pub ty: ValueType,
    pub span: Span,
}
#[derive(Clone, Debug)]
pub(crate) enum PatternKind {
    Any,
    Bind(Local),
    Literal(Literal),
    Tuple(Vec<Pattern>),
    Constructor {
        ctor: crate::contract::ir::CtorRef,
        fields: Vec<Pattern>,
    },
}
#[derive(Clone, Debug)]
pub(crate) struct Arm {
    pub pattern: Pattern,
    pub guard: Option<Expr>,
    pub value: Expr,
}

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
    Bind { local: Local, value: Expr },
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

use crate::Span;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Text {
    pub value: String,
    pub span: Span,
}
impl Text {
    pub fn new(value: impl Into<String>, span: Span) -> Self {
        Self {
            value: value.into(),
            span,
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Module {
    pub imports: Vec<Import>,
    pub functions: Vec<Function>,
}
#[derive(Clone, Debug)]
pub(crate) struct Import {
    pub path: Text,
    pub members: Option<Vec<Text>>,
}
#[derive(Clone, Debug)]
pub(crate) struct Function {
    pub name: Text,
    pub public: bool,
    pub params: Vec<Parameter>,
    pub return_type: Option<Text>,
    pub attributes: Vec<Attribute>,
    pub body: Option<Block>,
}
#[derive(Clone, Debug)]
pub(crate) struct Parameter {
    pub name: Text,
    pub ty: Text,
}
#[derive(Clone, Debug)]
pub(crate) enum Attribute {
    Export { target: Text, symbol: Text },
    External { target: Text, symbol: Text },
    Intrinsic(Span),
}
#[derive(Clone, Debug)]
pub(crate) struct Block {
    pub statements: Vec<Statement>,
    pub span: Span,
}
#[derive(Clone, Debug)]
pub(crate) enum Statement {
    Bind {
        name: Text,
        annotation: Option<Text>,
        value: Expr,
    },
    Expr(Expr),
}
#[derive(Clone, Debug)]
pub(crate) struct Expr {
    pub kind: ExprKind,
    pub span: Span,
}
#[derive(Clone, Debug)]
pub(crate) enum ExprKind {
    Integer(String),
    Float(String),
    String,
    Bool(bool),
    Unit,
    Name(Vec<Text>),
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
        callee: Box<Expr>,
        args: Vec<Expr>,
    },
    If {
        condition: Box<Expr>,
        then: Block,
        else_: Option<Block>,
    },
    Block(Block),
    Assert(Box<Expr>),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Unary {
    Neg,
    Not,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Binary {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
    Pipe,
}
impl Binary {
    pub fn comparison(self) -> bool {
        matches!(
            self,
            Self::Eq | Self::Ne | Self::Lt | Self::Le | Self::Gt | Self::Ge
        )
    }
}

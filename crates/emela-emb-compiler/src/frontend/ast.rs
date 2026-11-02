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
    pub data: Vec<Data>,
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
    pub type_params: Vec<TypeParameter>,
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
        pattern: Pattern,
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
    String(String),
    Interpolate(Vec<Expr>),
    Tuple(Vec<Expr>),
    Match {
        value: Box<Expr>,
        arms: Vec<Arm>,
    },
    Field {
        base: Box<Expr>,
        name: Text,
    },
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
        args: Vec<Argument>,
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

#[derive(Clone, Debug)]
pub(crate) struct Argument {
    pub label: Option<Text>,
    pub value: Expr,
}

#[derive(Clone, Debug)]
pub(crate) struct TypeParameter {
    pub name: Text,
    pub bounds: Vec<Text>,
}
#[derive(Clone, Debug)]
pub(crate) struct Data {
    pub name: Text,
    pub public: bool,
    pub opaque: bool,
    pub error: bool,
    pub params: Vec<TypeParameter>,
    pub variants: Vec<Variant>,
    pub derives: Vec<Text>,
}
#[derive(Clone, Debug)]
pub(crate) struct Variant {
    pub name: Text,
    pub fields: Vec<Field>,
}
#[derive(Clone, Debug)]
pub(crate) struct Field {
    pub name: Option<Text>,
    pub ty: Text,
}

#[derive(Clone, Debug)]
pub(crate) struct Pattern {
    pub kind: PatternKind,
    pub span: Span,
}
#[derive(Clone, Debug)]
pub(crate) enum PatternKind {
    Any,
    Bind(Text),
    Literal(Box<Expr>),
    Tuple(Vec<Pattern>),
    Constructor {
        path: Vec<Text>,
        args: Vec<PatternArgument>,
        rest: bool,
    },
}
#[derive(Clone, Debug)]
pub(crate) struct PatternArgument {
    pub label: Option<Text>,
    pub value: Pattern,
}
#[derive(Clone, Debug)]
pub(crate) struct Arm {
    pub pattern: Pattern,
    pub guard: Option<Expr>,
    pub value: Expr,
}

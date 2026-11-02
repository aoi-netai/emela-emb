use crate::Span;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Text<'a> {
    pub value: &'a str,
    pub span: Span,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Module<'a> {
    pub function: Function<'a>,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Function<'a> {
    pub export: Export<'a>,
    pub name: Text<'a>,
    pub return_type: Text<'a>,
    pub value: Text<'a>,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Export<'a> {
    pub target: Text<'a>,
    pub symbol: Text<'a>,
}

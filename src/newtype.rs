//! Wrapper types for every primitive value this tool handles.
//!
//! This module holds data only. A caller reads a wrapped value through the
//! public field. No function here computes anything.
//!
//! The tool wraps every primitive so that the compiler rejects a swapped
//! argument. A `CatName` never reaches a parameter that expects a `FunName`,
//! and a `DeBruijnIndex` never reaches a parameter that expects a
//! `DeBruijnLevel`.

/// Declares a wrapper around `String`.
macro_rules! text_newtype {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(pub String);
    };
}

/// Declares a wrapper around a number.
macro_rules! count_newtype {
    ($(#[$doc:meta])* $name:ident, $inner:ty) => {
        $(#[$doc])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(pub $inner);
    };
}

text_newtype! {
    /// The name of a grammar module, such as `Logic`.
    ModuleName
}

text_newtype! {
    /// The name of a category declared with `cat`, such as `Prop`.
    CatName
}

text_newtype! {
    /// The name of a constructor declared with `fun`, such as `Every`.
    FunName
}

text_newtype! {
    /// The name a binder writes down, such as `x` in `\x -> Barks x`.
    VarName
}

text_newtype! {
    /// The name of a parameter type declared with `param`, such as `Num`.
    ParamTypeName
}

text_newtype! {
    /// The name of one value of a parameter type, such as `Sg`.
    ParamValue
}

text_newtype! {
    /// The name of a record field, such as `s`.
    FieldName
}

text_newtype! {
    /// The name of a helper declared with `oper`.
    OperName
}

text_newtype! {
    /// One word of running text after tokenization.
    Token
}

text_newtype! {
    /// The full text of one grammar source file.
    SourceText
}

text_newtype! {
    /// The full text a caller asks the tool to read.
    InputText
}

count_newtype! {
    /// A level in the universe hierarchy. `Type 0` inhabits `Type 1`.
    UniverseLevel, u32
}

count_newtype! {
    /// A variable counted from the binder inwards. `Var(0)` names the nearest binder.
    DeBruijnIndex, usize
}

count_newtype! {
    /// A variable counted from the outside in. Normalization uses levels.
    DeBruijnLevel, usize
}

count_newtype! {
    /// The position of one argument in a `fun` declaration, counted from zero.
    ArgIndex, usize
}

count_newtype! {
    /// The identity of one compiled context-free production.
    ProductionId, usize
}

count_newtype! {
    /// A position between two input words. Position zero precedes the first word.
    ChartPos, usize
}

count_newtype! {
    /// How many symbols of a production the parser has already matched.
    DotPos, usize
}

count_newtype! {
    /// A line in a grammar source file, counted from one.
    Line, u32
}

count_newtype! {
    /// A column in a grammar source file, counted from one.
    Column, u32
}

count_newtype! {
    /// How deep a recursive function may still descend. Zero stops it.
    EvalFuel, u32
}

count_newtype! {
    /// How many trees the tool returns for one sentence before it stops.
    TreeLimit, usize
}

count_newtype! {
    /// How many parameter states one linearization type may hold.
    StateLimit, usize
}

count_newtype! {
    /// The position of one sentence in the input text, counted from zero.
    SentenceIndex, usize
}

count_newtype! {
    /// The identity of one nonterminal of the compiled grammar.
    NonterminalId, usize
}

count_newtype! {
    /// The identity of one grammar word in the parsing tables.
    TerminalId, usize
}

count_newtype! {
    /// How many productions the compiled grammar may hold.
    ProductionLimit, usize
}

count_newtype! {
    /// How many distinct missing phrases the compiler may thread.
    GapLimit, usize
}

count_newtype! {
    /// A position on the right side of a production, counted from zero.
    RhsIndex, usize
}

count_newtype! {
    /// The position of one item inside one chart column.
    ItemSlot, usize
}

count_newtype! {
    /// A place in the input text, counted in characters from the start.
    TextOffset, usize
}

count_newtype! {
    /// How many words one sentence may hold before the tool stops.
    SentenceLimit, usize
}

count_newtype! {
    /// How many items one chart column may hold before the tool stops.
    ItemLimit, usize
}

count_newtype! {
    /// How many ways of splitting one production the tool follows.
    SplitLimit, usize
}

count_newtype! {
    /// How many rejected trees the tool keeps in a report for one sentence.
    RejectionLimit, usize
}

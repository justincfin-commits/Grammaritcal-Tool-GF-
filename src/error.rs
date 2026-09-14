//! Error and diagnostic data.
//!
//! This module holds data only. The module `crate::render` turns this data into
//! text. Every error names the thing that failed. No error carries a
//! pre-formatted sentence.
//!
//! Every variant below carries a documentation line. The fields inside a
//! variant name themselves, so this module turns off the field level
//! documentation warning.

#![allow(missing_docs)]

use crate::concrete::lintype::{FieldPath, ParamState, ValueShape};
use crate::mltt::term::{ConstName, Term};
use crate::newtype::{
    ArgIndex, CatName, ChartPos, Column, DeBruijnIndex, FieldName, FunName, GapLimit, ItemLimit,
    Line, ModuleName, OperName, ParamTypeName, ParamValue, ProductionId, ProductionLimit,
    SentenceIndex, SentenceLimit, SplitLimit, StateLimit, Token, VarName,
};

/// A position in a grammar source file. Lines and columns count from one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SourcePos {
    /// The line.
    pub line: Line,
    /// The column.
    pub column: Column,
}

/// A failure while the tool splits a grammar source file into words.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LexError {
    /// The file holds a character that no token may contain.
    UnexpectedCharacter { pos: SourcePos, found: char },
    /// A string literal reaches the end of the file without a closing quote.
    UnterminatedString { pos: SourcePos },
    /// A block comment reaches the end of the file without a closing mark.
    UnterminatedComment { pos: SourcePos },
}

/// A failure while the tool reads the shape of a grammar source file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParseError {
    /// The parser wanted one thing and found another.
    Expected {
        pos: SourcePos,
        expected: String,
        found: String,
    },
    /// The file ended while the parser still wanted something.
    UnexpectedEnd { expected: String },
    /// The file nests brackets deeper than the parser accepts.
    NestingTooDeep { pos: SourcePos },
    /// The concrete module names an abstract module that the caller did not supply.
    AbstractNameMismatch {
        pos: SourcePos,
        expected: ModuleName,
        found: ModuleName,
    },
    /// Two declarations claim the same name.
    DuplicateDeclaration { pos: SourcePos, name: String },
}

/// A failure while the tool turns declarations into signature entries.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LowerError {
    /// A type or a term names something the grammar never declares.
    UnknownIdentifier { name: String },
    /// The grammar declares one name twice.
    DuplicateDeclaration { name: String },
    /// The abstract module names a start category that it never declares.
    UnknownStartCategory { name: CatName },
    /// The abstract module declares no start category and the caller supplied none.
    MissingStartCategory,
    /// A term nests deeper than the tool accepts.
    NestingTooDeep,
}

/// A failure while the type checker reads a term.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TypeError {
    /// A recursive function reached its depth limit.
    OutOfFuel,
    /// A term names a variable that no binder introduces.
    UnboundVariable { index: DeBruijnIndex },
    /// A term names a constant that the signature never declares.
    UnknownConstant { name: ConstName },
    /// A term applies something that is not a function.
    NotAFunction { found_type: Term },
    /// A term projects from something that is not a pair.
    NotAPair { found_type: Term },
    /// A term stands where a type belongs, and it is not a type.
    NotAType { found_type: Term },
    /// A term eliminates something that is not an identity proof.
    NotAnIdentity { found_type: Term },
    /// The motive of an identity elimination has the wrong shape.
    BadMotive { found_type: Term },
    /// The checker found one type where it expected another.
    Mismatch { expected: Term, found: Term },
    /// The checker cannot read a type off this term without more information.
    CannotInfer { subject: Term },
    /// A function stands where a value of another type belongs.
    LambdaAgainstNonFunction { expected: Term },
    /// A pair stands where a value of another type belongs.
    PairAgainstNonPairType { expected: Term },
    /// Evaluation applied a value that is not a function. A well typed term
    /// never reaches this failure.
    AppliedNonFunction,
    /// Evaluation projected from a value that is not a pair. A well typed term
    /// never reaches this failure.
    ProjectedNonPair,
    /// Evaluation eliminated a value that is not an identity proof. A well
    /// typed term never reaches this failure.
    EliminatedNonProof,
}

/// A failure while the tool evaluates a linearization expression.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConcreteError {
    /// A recursive function reached its depth limit.
    OutOfFuel,
    /// An expression names something the concrete module never declares.
    UnknownIdentifier { name: String },
    /// An expression projects a field from something that is not a record.
    NotARecord { found: ValueShape },
    /// An expression projects a field that the record does not hold.
    MissingField {
        field: FieldName,
        available: Vec<FieldName>,
    },
    /// An expression selects from something that is not a table.
    NotATable { found: ValueShape },
    /// An expression selects a row that the table does not hold.
    MissingRow { value: ParamValue },
    /// An expression selects with something that is not a parameter value.
    NotAParameter { found: ValueShape },
    /// An expression joins something that is not a string.
    NotAString { found: ValueShape },
    /// An expression applies something that is not a function.
    NotAFunction { found: ValueShape },
    /// A helper received the wrong number of arguments.
    WrongArity {
        name: OperName,
        expected: usize,
        found: usize,
    },
    /// A pattern names a parameter value that no parameter type declares.
    UnknownParamValue { value: ParamValue },
    /// A table or a case covers no branch for the value the tool selected.
    NoBranchMatches { value: ParamValue },
    /// A record repeats one field name.
    DuplicateField { field: FieldName },
    /// A table holds no named row, so the tool cannot tell which parameter
    /// type it covers.
    TableNeedsNamedRow,
    /// A linearization type names a parameter type the module never declares.
    UnknownParamTypeName { name: ParamTypeName },
    /// A value does not match the linearization type the category declares.
    ShapeMismatch { found: ValueShape },
    /// A rule uses a higher-order argument without applying it to a record.
    BinderNotApplied { arg: ArgIndex },
    /// A rule applies a higher-order argument to a record that prints words.
    GapNotEmpty { arg: ArgIndex },
}

/// Why the compiler refuses an argument type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UnsupportedReason {
    /// The argument type is a function whose result is itself a function.
    NestedFunction,
    /// The argument type is neither a category nor a function into a category.
    NotACategory,
    /// The argument type is a universe, which no word sequence can name.
    Universe,
    /// The argument type is a pair type or an identity type.
    NotSupportedByParser,
}

/// A failure while the tool turns a concrete module into parsing rules.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CompileError {
    /// The concrete module declares no linearization type for a category.
    MissingLinCat { cat: CatName },
    /// The concrete module declares no linearization rule for a constructor.
    MissingLin { fun: FunName },
    /// A rule binds a different number of names than the constructor takes arguments.
    LinArityMismatch {
        fun: FunName,
        expected: usize,
        found: usize,
    },
    /// A linearization type names a parameter type the module never declares.
    UnknownParamType { name: ParamTypeName },
    /// Two parameter types declare the same value name.
    DuplicateParamValue { value: ParamValue },
    /// One output string of a rule never mentions an argument, so the parser
    /// could never recover that argument from text.
    ArgumentDropped {
        fun: FunName,
        arg: ArgIndex,
        path: FieldPath,
    },
    /// One output string of a rule mentions an argument more than once, which
    /// no context-free production can express.
    ArgumentRepeated {
        fun: FunName,
        arg: ArgIndex,
        path: FieldPath,
    },
    /// The type of an argument lies outside the fragment the parser compiles.
    UnsupportedArgumentType {
        fun: FunName,
        arg: ArgIndex,
        reason: UnsupportedReason,
    },
    /// The result type of a constructor is not an application of a category.
    UnsupportedResultType { fun: FunName },
    /// A rule produces a value whose shape does not match the declared
    /// linearization type of the result category.
    ResultShapeMismatch {
        fun: FunName,
        cat: CatName,
        found: ValueShape,
    },
    /// One category carries more parameter states than the tool accepts.
    StateExplosion {
        cat: CatName,
        count: usize,
        limit: StateLimit,
    },
    /// Evaluating a rule failed.
    Evaluation { fun: FunName, error: ConcreteError },
    /// The compiled grammar holds more productions than the tool accepts.
    TooManyProductions { count: usize, limit: ProductionLimit },
    /// The concrete module declares two linearization types for one category.
    DuplicateLinCat { cat: CatName },
    /// The concrete module declares two rules for one constructor.
    DuplicateLin { fun: FunName },
    /// A rule names a constructor the abstract module never declares.
    LinForUnknownFunction { fun: FunName },
    /// A linearization type names a category the abstract module never declares.
    LinCatForUnknownCategory { cat: CatName },
    /// No part of a category prints words, so no sentence can carry it.
    CategoryPrintsNothing { cat: CatName },
    /// One rule fixes more combinations of feature values than the tool accepts.
    RuleStateExplosion {
        fun: FunName,
        count: usize,
        limit: StateLimit,
    },
    /// The grammar asks for more distinct missing phrases than the tool accepts.
    TooManyGaps { count: usize, limit: GapLimit },
    /// A part of the compiler reached its depth limit.
    OutOfFuel { stage: CompileStage },
    /// One nonterminal derives itself with no words in between, so the number
    /// of derivations of some sentence is unbounded.
    UnitCycle {
        cat: CatName,
        state: ParamState,
        path: FieldPath,
    },
    /// The nullable computation did not settle. This failure reports a defect
    /// inside the tool.
    NullableUnstable,
}

/// Which part of the compiler ran out of its depth budget.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompileStage {
    /// Walking the declared type of a constructor.
    ArgumentTypes,
    /// Walking the value a rule produced.
    ResultValue,
    /// Removing productions that derive nothing or that nothing reaches.
    Pruning,
}

/// A failure while the tool reads running text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReadError {
    /// The input text holds no sentence.
    EmptyInput,
    /// One sentence holds more words than the tool accepts.
    SentenceTooLong {
        sentence: SentenceIndex,
        length: usize,
        limit: SentenceLimit,
    },
    /// No derivation covers the sentence, and no word was matched at all.
    NoParse,
    /// No derivation covers the sentence. The parser got as far as the named
    /// position, where it could have accepted one of the listed words.
    NoParseAt {
        position: ChartPos,
        expected: Vec<Token>,
    },
    /// The sentence holds a word that no rule in the grammar prints.
    UnknownWord { token: Token },
    /// The sentence holds a word that the grammar prints only with different
    /// capitals.
    UnknownWordCaseOnly {
        token: Token,
        grammar_spelling: Token,
    },
    /// The parser stopped after it reached the tree limit before it found a
    /// well typed tree.
    TreeLimitReached { returned: usize },
    /// One chart column grew past the limit.
    ChartTooLarge { position: ChartPos, limit: ItemLimit },
    /// One production over one stretch of the sentence can be split in more
    /// ways than the tool follows.
    TooManySplits {
        production: ProductionId,
        limit: SplitLimit,
    },
    /// The tree rebuilder reached its depth limit.
    OutOfFuel,
    /// A function argument received a phrase with the wrong number of places
    /// for its bound variable. This failure reports a defect inside the tool.
    WrongTraceCount {
        fun: FunName,
        arg: ArgIndex,
        found: usize,
    },
    /// A bound variable stands where no function argument binds it. This
    /// failure reports a defect inside the tool.
    TraceOutsideBinder { cat: CatName, state: ParamState },
    /// A derivation names a production the compiled grammar does not hold.
    /// This failure reports a defect inside the tool.
    BrokenDerivation,
}

/// A failure while the tool prints a tree back as words.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WriteError {
    /// The tree applies a constructor the grammar never declares.
    UnknownConstructor { fun: FunName },
    /// The tree is not an application of a constructor.
    NotAConstructorTree,
    /// The tree supplies the wrong number of arguments.
    WrongArity {
        fun: FunName,
        expected: usize,
        found: usize,
    },
    /// Printing a rule failed.
    Evaluation { fun: FunName, error: ConcreteError },
    /// The printed value carries a parameter state the caller did not ask for.
    NoSuchState { cat: CatName, state: ParamState },
    /// A variable stands outside any binder.
    LooseVariable { name: VarName },
    /// The tool reached its depth limit while printing.
    OutOfFuel,
}

/// Any failure the tool reports.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GfError {
    /// A failure while splitting a grammar file into words.
    Lex(LexError),
    /// A failure while reading the shape of a grammar file.
    Parse(ParseError),
    /// A failure while building signature entries.
    Lower(LowerError),
    /// A failure while type checking.
    Type(TypeError),
    /// A failure while evaluating a linearization rule.
    Concrete(ConcreteError),
    /// A failure while building parsing rules.
    Compile(CompileError),
    /// A failure while reading running text.
    Read(ReadError),
    /// A failure while printing a tree.
    Write(WriteError),
}

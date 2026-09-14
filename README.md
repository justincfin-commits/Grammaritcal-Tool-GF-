# gf-mltt

A tool that reads English text and produces dependently typed abstract syntax
trees. It keeps the split that Grammatical Framework (GF) makes between an
*abstract syntax* (categories and typed constructors) and a *concrete syntax*
(how each constructor prints as words), and it replaces GF's abstract type
system with full Martin-Löf type theory. The trees it returns are terms of
that type theory, each paired with its type in normal form, ready for a later
stage to hand to a theorem prover.

The library exposes one pure function:

```rust
pub fn analyze(request: &AnalysisRequest) -> AnalysisOutcome
```

It takes grammar text and input text as data and returns trees and
diagnostics as data. It reads no file, opens no socket, reads no clock, draws
no random number, and mutates no global state. The command line program
`gfmltt` does every file read and every print, and calls `analyze` once.

Standard library only. No dependencies. No `unsafe`.

## What it does, in one run

```
$ gfmltt --abstract grammars/Logic.gf --concrete grammars/LogicEng.gf --text "Every dog barks. Every rock barks."
grammar Logic / LogicEng: start category Prop, 22 constants, 9 nonterminals, 41 productions (25 before gap threading), 2 gaps, 27 words
sentence 1: Every dog barks
  1 reading(s)
  [1] Every Animate dog (\x -> Barks x)
      : Prop
  (1 candidate subtree(s) refused by the type checker on the way)
sentence 2: Every rock barks
  no reading: 2 candidate tree(s) parsed and every one failed to type check
  rejected Every Animate rock (\x -> Barks x)
      because expected type CN Animate, found type CN Thing
  rejected Every Thing rock (\x -> Barks x)
      because expected type Ind Animate, found type Ind Thing
```

Sentence 2 is the point of the design. The context-free stage builds two
trees for it, one per sort. Neither survives the type checker: `rock : CN
Thing` forces the sort to `Thing`, and `Barks` demands `Ind Animate`. The
second rejection is the one no parser could produce on its own: the *bound
variable* `x` has the wrong type index, not any word.

## The five stages

Every call of `analyze` runs these in order. Each stage takes data from the
one before and returns data to the one after.

| Stage | Module | What it does |
| --- | --- | --- |
| 1 | `src/gf/lexer.rs`, `src/gf/parser.rs` | Reads the abstract and the concrete grammar text into declarations. |
| 2 | `src/gf/lower.rs`, `src/mltt/signature.rs` | Turns `cat` and `fun` declarations into constants of the type theory, then checks that every declared type is a type. A misspelled category fails here. |
| 3 | `src/compile/productions.rs`, `src/compile/gaps.rs` | Turns each `lin` rule into context-free productions. A nonterminal is the four part key (category, parameter state, field path, missing phrase). |
| 4 | `src/parse/tokenize.rs`, `src/parse/earley.rs`, `src/parse/forest.rs` | Splits the input into sentences, runs an Earley chart parser, and rebuilds candidate trees from the chart. |
| 5 | `src/mltt/check.rs` | Runs `infer` on every candidate. Only well typed trees reach the output, each with its normalized type. |

The type theory core (`src/mltt/`) is intensional Martin-Löf type theory with
de Bruijn indices, a universe hierarchy `Type : Type1 : Type2 …`, dependent
function types, dependent pair types, and the identity type with `refl` and
the `J` eliminator. Judgmental equality runs by normalization by evaluation:
`eval` turns a term into a value, `quote` turns a value back into a normal
term, `convertible` compares two values. Grammar constructors stay opaque, so
normalization always terminates.

## Step by step: using the tool

### 1. Build it

```bash
cargo build --release
```

The binary is `target/release/gfmltt.exe` (or `target/release/gfmltt` on
other systems). Run the tests with:

```bash
cargo test
```

### 2. Pick or write a grammar pair

A grammar is two files. The abstract file declares the categories and the
typed constructors. The concrete file says how each constructor prints.

Two pairs ship in `grammars/`:

| Pair | Covers |
| --- | --- |
| `Logic.gf` / `LogicEng.gf` | A small logic fragment: proper nouns, common nouns with number, quantifiers (`every`, `all`, `some`) that build lambda terms, `and`, `or`, `not`. Shows the gap mechanism. |
| `DynamicControlPlane.gf` / `DynamicControlPlaneEng.gf` | A requirements fragment whose vocabulary comes from the claims of WO 2025/075926. Determiners, multiword nouns, an `of` modifier, the RFC 2119 modals `MUST`, `MUST NOT`, `SHOULD`, `SHOULD NOT`, `MAY`, active verbs, passive verbs with an optional agent, and predicate adjectives. Requirements are indexed by modal strength: `cat Req (m : Modality)`. |

### 3. Write the input text

Any plain text file. The tool splits it into sentences at `.`, `?`, `!`, and
blank lines, and reads each sentence on its own. Every word must be a word
the grammar prints; an unknown word is reported by name.

### 4. Run it

```bash
gfmltt --abstract grammars/DynamicControlPlane.gf --concrete grammars/DynamicControlPlaneEng.gf --file tests/data/acceptance.txt
```

Or pass the text on the command line:

```bash
gfmltt --abstract grammars/Logic.gf --concrete grammars/LogicEng.gf --text "All dogs bark and John sleeps."
```

Every flag takes one value:

| Flag | Meaning |
| --- | --- |
| `--abstract A.gf` | The abstract grammar file. Required. |
| `--concrete AEng.gf` | The concrete grammar file. Required. |
| `--text "…"` or `--file in.txt` | The input. One of the two is required. |
| `--start Cat` | The category a sentence must produce. Optional; the abstract file's `flags startcat` line is the default. |
| `--dcbor out.cbor` | Also write the whole outcome as deterministic CBOR bytes. |
| `--diag out.diag` | Also write the whole outcome in CBOR diagnostic notation (human readable). |
| `--vectors DIR` | Also write one `NNN.cbor` and one `NNN.diag` per sentence into `DIR`. These are the exportable test vectors. |

Exit code: `0` when every sentence has at least one reading, `1` when some
sentence has none, `2` on a usage error or an unreadable file.

### 5. Read the report

For each sentence the report shows the words, then each accepted reading as a
term with its type, or the reason nothing was accepted. Three kinds of
failure look different on purpose:

| Report line | Meaning |
| --- | --- |
| `no reading: the word "zebra" appears in no rule of the grammar` | Add the word to the grammar (see step 6). |
| `no reading: the grammar covers the first 2 words and then wanted one of: …` | The sentence shape is not in the grammar. The parser got as far as it could. |
| `no reading: N candidate tree(s) parsed and every one failed to type check`, followed by `rejected … because expected type …, found type …` | The sentence is well formed but ill typed. This is the dependent type filter doing its work. |

When a reading exists, the subtrees the type checker refused on the way are
counted, not listed. When no reading exists, they are listed, because they
are the evidence.

### 6. Extend a grammar for a new document

The grammar is yours to supply; the tool never infers one. One grammar
serves every document that shares its vocabulary and sentence shapes. To
cover a new document:

1. Run the tool on it. Note every `appears in no rule of the grammar` line.
2. For each new noun, add a `fun` line to the abstract file under the right
   sort, and a `lin` line to the concrete file with its singular and plural:
   ```
   fun auditLog : CN Resource ;                          -- abstract
   lin auditLog = noun "audit log" "audit logs" ;        -- concrete
   ```
3. For each new verb, decide what sorts its subject and object have:
   ```
   fun notify_V2 : V2 Actor Actor ;                      -- abstract
   lin notify_V2 = { s = "notify" } ;                    -- concrete
   ```
4. For a new sentence shape, add a constructor and a rule. The compiler
   refuses a rule that breaks a limit (below) and names the rule and the
   reason.
5. Rerun. Repeat until every sentence you care about has exactly one reading
   and every sentence that should fail does fail with a type error.

Keep sorts honest. The whole value of the tool is that "the credential MUST
determine the cloud platform" is *refused*, and it is refused only because
`determine_V2 : V2 Actor Resource` says who may determine what.

## The dialect

The tool reads a subset of GF with one extension.

Accepted in the abstract file: `abstract Name = { … }`, `flags startcat = C ;`,
`cat C ;`, `cat C (x : A) (y : B) ;`, `fun f, g : T ;`, dependent function
types `(x : A) -> B` and `A -> B`, application `f a b`, and the extension
below. Line comments `--` and block comments `{- -}`.

**The extension: `Type`, `Type1`, `Type2`, … are universes.** Standard GF has
no universe, so it cannot write `fun Forall : (A : Type) -> (A -> Prop) ->
Prop`. This dialect can, and the checker validates such a declaration with no
extra machinery, because quantifying over a type is ordinary dependent
quantification once a universe exists. Also accepted: dependent pair types
`(x : A) ** B`, pairs `<a, b>`, projections `fst`, `snd`, the identity type
`Id A a b`, `refl A a`, and the eliminator `J n A a m base b p`.

Accepted in the concrete file: `concrete Name of Abstract = { … }`, `param P
= V1 | V2 ;`, `lincat C = L ;`, `lin f x y = e ;`, `oper h x y = e ;`.
Linearization types: `Str`, a parameter type name, records `{ s : Str ; n :
Num }`, tables `Num => Str`. Expressions: `"words"`, `[]`, `++`, records,
projection `r.f`, tables `table { Sg => e ; Pl => e }`, selection `t ! v`,
`case e of { … }`, helper application. Selection reads one atom plus any
`.field` steps after `!`, so `t ! x.n` selects with `x.n`.

The category a `lin` rule produces is fixed by the `fun` declaration; the
compiler reads the head of the result type and ignores its index arguments.
`CN Animate` and `CN Thing` compile to one nonterminal family, which is why
the context-free stage over-generates and the type checker filters.

## The gap mechanism, and why the variable is always index zero

A quantifier such as `fun Every : (k : Sort) -> CN k -> (Ind k -> Prop) ->
Prop` takes a function argument. The compiler turns it into a nonterminal
that carries a *gap*: a `Prop` phrase missing one `Ind` phrase. The grammar
writer names the gap by applying the argument to a record of empty strings,
exactly as GF does:

```
lin Every k cn vp = { s = k.s ++ "every" ++ cn.s ! Sg ++ (vp { s = [] ; n = Sg }).s } ;
```

The record's parameter fields (`n = Sg`) say which state the missing phrase
carries. A record whose strings are not empty is refused, naming the rule.

Two production families make the gapped nonterminal derivable. One copies
each base production once per gap and per child that carries no gap, and
moves the gap onto that one child. One lets the gapped nonterminal of the
gap's own category match no words: that is where the bound variable stands.
The tree rebuilder wraps the gapped subtree in a lambda and writes the gap
position as de Bruijn variable zero.

Index zero is always right because the pass-down family never sends a gap
into a child that already carries one. A child carries a gap exactly when
its argument is a function, which is exactly where the rebuilder adds a
lambda. So no lambda ever stands between the gap site and the lambda that
binds it. `verify/src/gaps.rs` proves the "never into a gapped child" half
mechanically; `spec.pvs` states the rest.

## Limits

| Limit | Reason |
| --- | --- |
| Each argument must contribute its words exactly once per output field | Keeps the compiled grammar context free. Reduplication raises `ArgumentRepeated`; a dropped argument raises `ArgumentDropped`. Both name the rule, the argument, and the field. |
| A phrase carries at most one gap | Two nested gaps need a stack of missing phrases and a computed variable index. One gap covers quantifiers. Enforced by the type: `Nonterminal.gap` is an `Option`. |
| No quantifier inside another quantifier's scope, and no across-the-board extraction | Both need two gaps in one phrase. "Every dog barks and sleeps" does not parse; "every dog barks and John sleeps" gets two readings (both scopes). |
| Parameter types hold plain values only, such as `Sg` and `Pl` | Parameter constructors that take arguments multiply the state count. Not needed for the example grammars. |
| No `def` equations in the abstract syntax | Constants stay opaque, which keeps equality decidable without a termination check. |
| No `variants`, no `pre`, no `Strs`, no discontinuous linearization types | Each raises a named error at grammar load time rather than silently mis-parsing. Because there is no `pre`, `a` and `an` are two determiners in `DynamicControlPlane.gf`, and `an cloud platform` also parses. |
| The modal word lives in the `Prop` table, not in `Obligation` | A rule `s = pr.subj ++ mo.s ++ pr.pred` would mention `pr` twice in one field. So `Modality` prints nothing and carries a feature, `Prop` holds one complete statement per modal strength, and `Obligation` selects the row. A reader of `lin Obligation` will not find the word `MUST` in it. |
| Case: exact spelling first, ASCII lower-case second, never upward | `Every` matches `every`; `MUST` matches only `MUST`; `must` is reported as "appears in the grammar only as MUST". RFC 2119 capitals keep their meaning with no configuration. Non-ASCII case is not folded. |
| Sentence splitting is a fixed rule | `.`, `?`, `!` at the end of a word and blank lines end a sentence. `e.g.` ends a sentence. A missing space is not repaired. `;` does not end a sentence. |
| Every `Sort` argument doubles the raw candidate count | Both sort constructors print nothing. The rebuilder type checks every closed subtree as it builds it and drops failures, which keeps the count small in practice; this never changes the set of well typed trees. |
| The tool produces trees and types, not theorem prover input | A later stage turns the terms into whatever the prover reads. |

Every limit that can be hit is a named enum variant in `src/error.rs`, and
`src/render.rs` has one plain sentence per variant.

## The purity claim, and how it is held

**Claim.** The library performs no input and no output, and two calls of
`analyze` on equal requests return equal outcomes, byte for byte through the
renderers.

**By construction.** No `unsafe`, no `std::fs`, no `std::io`, no
`std::time`, no `std::net` in the library. No `unwrap`, `expect`, `panic!`,
`unreachable!`, `todo!`, and no indexing that can panic (clippy's
`indexing_slicing`, `unwrap_used`, `expect_used`, `panic`, `unreachable`,
`todo` lints are denied in `src/lib.rs`). Every recursive function carries
an explicit fuel counter and returns a named error when it runs out. No
`HashMap` or `HashSet` anywhere; every container is a `Vec`, `BTreeMap`, or
`BTreeSet`, so iteration order is a function of contents.

**By test.** `tests/purity.rs` reads every library source file and fails on
any forbidden token. `tests/determinism.rs` calls `analyze` twice on one
request and compares the outcomes as data, as text, and as dCBOR bytes.

**By formal specification.** `spec.pvs` states the data and the properties
in PVS. It is a blueprint: PVS is not installed on the machine this was built
on, so the file has not been type checked or proved by PVS.

**By machine checked proof, for models.** `verify/` is a separate crate of
Verus models of the core algorithms. It is not a member of the main build,
so the main crate stays dependency free. Verus 0.2026.06.14 verified it:

```
verification results:: 37 verified, 0 errors
```

| Module | Models | Proves |
| --- | --- | --- |
| `fuel.rs` | `spend` | fuel strictly decreases or the call stops; a fuelled recursion terminates |
| `debruijn.rs` | `quote_neutral`'s arithmetic | level-to-index is defined exactly when the level is under the depth, and inverts index-to-level |
| `linearity.rs` | `check_linearity` | accepts exactly when every argument appears once |
| `gaps.rs` | `pass_down` | every copy moves the gap onto exactly one child; that child carried no gap before; a copy has one more gap than the base |
| `fixpoint.rs` | `compute_nullable` | a round only turns flags on; a changing round raises the count; at most `n + 1` rounds run |
| `dcbor.rs` | `put_head`, `read_head` | the head is the shortest form for its value, and reading it back gives the major type and the value |

To rerun:

```bash
cd verify && C:\Users\Home\verus\verus-x86-win\cargo-verus.exe verify
```

## What is not claimed

- **Verus verifies models, not `src/` line for line.** The production code
  uses `Rc` and `Box` recursion that Verus does not take. The models are
  written to mirror the Rust step for step; the correspondence is by
  inspection and by shared test vectors, not by extraction.
- **`spec.pvs` is unchecked.** It is written to PVS's standard forms, but no
  PVS run has seen it. Lemmas marked `STATED` have a proof sketch in a
  comment and no machine proof anywhere. Lemmas marked `PROVED-IN-VERUS`
  have a machine proof of the corresponding Verus model.
- **The dCBOR encoder is not EverCBOR.** `src/dcbor.rs` is a hand written
  encoder and decoder that follows RFC 8949 section 4.2.1 and the dCBOR
  profile: shortest integer heads, definite lengths only, map keys sorted by
  their encoded bytes, no duplicate keys, no floating point. `tests/dcbor.rs`
  holds the vectors from RFC 8949 appendix A. `--vectors DIR` writes every
  sentence's encoding as `.cbor` bytes and `.diag` text so that a later check
  against EverCBOR or any other implementation stays possible. No such check
  has been run.
- **Prusti was not used.** It is not installed here and needs a crate
  dependency, which the zero-dependency rule forbids.

## The requirements grammar and its acceptance set

`tests/data/acceptance.txt` holds seven sentences derived from the claims of
WO 2025/075926. Each returns exactly one reading; `tests/control_plane.rs`
pins the exact tree and type of every one.

| Sentence | Type |
| --- | --- |
| The cloud platform MUST determine the specification template. | `Req MUST` |
| All API calls MUST be authenticated by the provider. | `Req MUST` |
| The credential SHOULD be stored. | `Req SHOULD` |
| The cloud platform MUST NOT redirect the developer of the application. | `Req MUST_NOT` |
| An account of the application MAY be configured. | `Req MAY` |
| The provider SHOULD NOT store the credential. | `Req SHOULD_NOT` |
| Every specification template MUST be common. | `Req MUST` |

`tests/data/rejected.txt` holds three sentences that parse and then fail to
type check. The first is the case the design calls for: a message verb
applied to an actor.

| Sentence | Why it is refused |
| --- | --- |
| The cloud platform MUST authenticate the provider. | `authenticate_V2 : V2 Actor Message`; "the provider" is `NP Actor`. |
| The credential MUST determine the cloud platform. | `determine_V2` wants an `Actor` subject; "the credential" is a `Resource`. |
| The API call MUST be common. | `common_AP : AP Resource`; "the API call" is a `Message`. |

Run both:

```bash
gfmltt --abstract grammars/DynamicControlPlane.gf --concrete grammars/DynamicControlPlaneEng.gf --file tests/data/acceptance.txt
```

```bash
gfmltt --abstract grammars/DynamicControlPlane.gf --concrete grammars/DynamicControlPlaneEng.gf --file tests/data/rejected.txt
```

The first exits `0`; the second exits `1` and prints every rejection with its
type error.

## Layout

| Path | Holds |
| --- | --- |
| `src/newtype.rs` | A distinct wrapper type for every primitive. Data only. |
| `src/error.rs` | Every error, as named enum variants. Data only. |
| `src/mltt/term.rs`, `value.rs`, `signature.rs` | Terms, semantic values, and the signature. Data, plus the one signature check. |
| `src/mltt/eval.rs`, `check.rs` | Normalization by evaluation; bidirectional `infer` and `check`. |
| `src/gf/token.rs`, `ast.rs` | Token and grammar data. |
| `src/gf/lexer.rs`, `parser.rs`, `lower.rs` | Tokenizer, recursive descent parser with a depth limit, lowering to the signature. |
| `src/concrete/lintype.rs`, `value.rs` | Linearization types, field paths, parameter states; concrete values with slots. Data. |
| `src/concrete/shape.rs`, `eval.rs` | Walks over linearization types; the linearization evaluator, driven by fuel. |
| `src/compile/grammar.rs` | The compiled grammar. Data. |
| `src/compile/productions.rs`, `gaps.rs` | Rule compilation and gap threading. |
| `src/parse/input.rs`, `chart.rs` | Sentence, chart, and forest data. |
| `src/parse/tokenize.rs`, `earley.rs`, `forest.rs` | Tokenizer, Earley recognizer with completion records, tree rebuilder with the early type filter. |
| `src/analyze.rs` | The request and outcome data, and the one pure function. |
| `src/dcbor.rs`, `src/render.rs` | Deterministic CBOR; text, dCBOR, and diagnostic renderers. |
| `src/bin/gfmltt.rs` | The only module that performs input and output. |
| `grammars/` | The two example grammar pairs. |
| `tests/` | Integration tests, including the acceptance sets. |
| `spec.pvs` | The PVS blueprint. |
| `verify/` | The Verus models and proofs. |

Data lives in the data modules as plain structs and enums with public fields
and derived traits. Behavior lives in the other modules as free functions
that take data and return data. No data module carries a method that
computes anything.

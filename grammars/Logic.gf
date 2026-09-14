-- The abstract syntax of a small English fragment.
--
-- Every declaration below is a constant of the core type theory. A cat
-- declaration builds a constant whose type ends in a universe. A fun
-- declaration builds a constant whose type the checker reads as written.
--
-- The category Ind is indexed by a sort, so the type of an individual records
-- whether that individual is animate. A verb that demands an animate subject
-- therefore rejects a rock at type checking time, not at parsing time.

abstract Logic = {

  flags startcat = Prop ;

  cat Sort ;
  cat Prop ;
  cat Ind (s : Sort) ;
  cat CN (s : Sort) ;

  fun Animate : Sort ;
  fun Thing : Sort ;

  fun dog, cat_ : CN Animate ;
  fun rock : CN Thing ;

  fun John, Fido : Ind Animate ;
  fun TheRock : Ind Thing ;

  fun Barks, Sleeps, Happy : Ind Animate -> Prop ;
  fun Falls : (k : Sort) -> Ind k -> Prop ;

  -- The third argument of a quantifier is a function. The parser builds that
  -- function from text, so "every dog barks" produces a lambda term.
  fun Every : (k : Sort) -> CN k -> (Ind k -> Prop) -> Prop ;
  fun All   : (k : Sort) -> CN k -> (Ind k -> Prop) -> Prop ;
  fun Some_ : (k : Sort) -> CN k -> (Ind k -> Prop) -> Prop ;

  fun And, Or : Prop -> Prop -> Prop ;
  fun Not : Prop -> Prop ;
}

-- The concrete syntax of the small English fragment.
--
-- A lincat declaration says what shape the words of one category take. The
-- category Ind carries a number feature that the words do not show, and the
-- category CN carries one word list per number.
--
-- A quantifier applies its third argument to a record of empty words. That
-- record names the bound variable, so the compiler knows which category the
-- phrase misses and which number that missing phrase carries.

concrete LogicEng of Logic = {

  param Num = Sg | Pl ;

  lincat Sort = { s : Str } ;
  lincat Prop = { s : Str } ;
  lincat Ind  = { s : Str ; n : Num } ;
  lincat CN   = { s : Num => Str } ;

  oper noun sg pl = { s = table { Sg => sg ; Pl => pl } } ;
  oper agree sg pl x = x.s ++ (table { Sg => sg ; Pl => pl } ! x.n) ;

  lin Animate = { s = [] } ;
  lin Thing   = { s = [] } ;

  lin dog  = noun "dog" "dogs" ;
  lin cat_ = noun "cat" "cats" ;
  lin rock = noun "rock" "rocks" ;

  lin John    = { s = "john" ; n = Sg } ;
  lin Fido    = { s = "fido" ; n = Sg } ;
  lin TheRock = { s = "the rock" ; n = Sg } ;

  lin Barks x  = { s = agree "barks" "bark" x } ;
  lin Sleeps x = { s = agree "sleeps" "sleep" x } ;
  lin Happy x  = { s = agree "is happy" "are happy" x } ;
  lin Falls k x = { s = k.s ++ agree "falls" "fall" x } ;

  lin Every k cn vp = { s = k.s ++ "every" ++ cn.s ! Sg ++ (vp { s = [] ; n = Sg }).s } ;
  lin All   k cn vp = { s = k.s ++ "all"   ++ cn.s ! Pl ++ (vp { s = [] ; n = Pl }).s } ;
  lin Some_ k cn vp = { s = k.s ++ "some"  ++ cn.s ! Sg ++ (vp { s = [] ; n = Sg }).s } ;

  lin And p q = { s = p.s ++ "and" ++ q.s } ;
  lin Or  p q = { s = p.s ++ "or"  ++ q.s } ;
  lin Not p   = { s = "it is not the case that" ++ p.s } ;
}

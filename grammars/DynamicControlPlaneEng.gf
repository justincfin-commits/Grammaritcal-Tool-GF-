-- The concrete syntax of the requirements fragment.
--
-- Where the modal word lives. A reader of "lin Obligation" will not find the
-- word MUST anywhere in it. That is forced by the compiler's one rule: each
-- argument contributes its words exactly once per output field. A rule such
-- as "s = pr.subj ++ mo.s ++ pr.pred" would mention pr twice, so it is
-- refused. Instead, Modality prints nothing and carries a feature m, Prop is
-- a table with one complete statement per modal strength, each row already
-- holding its modal word between subject and predicate, and Obligation reads
-- m off its Modality argument and selects that row. This is the same shape
-- LogicEng uses for number agreement.
--
-- Every Sort argument prints nothing but still appears once, as k.s or j.s,
-- so the parser can recover it and the type checker can judge it.
--
-- The modal words are written in capitals, as RFC 2119 writes them. The
-- parser matches the exact spelling first and folds capitals only when the
-- exact spelling is not a grammar word, so "must" in running prose does not
-- match "MUST".

concrete DynamicControlPlaneEng of DynamicControlPlane = {

  param Modal = Must | MustNot | Should | ShouldNot | May ;
  param Num = Sg | Pl ;

  lincat Sort     = { s : Str } ;
  lincat Modality = { s : Str ; m : Modal } ;
  lincat Prop     = { s : Modal => Str } ;
  lincat Req      = { s : Str } ;
  lincat Det      = { s : Str ; n : Num } ;
  lincat CN       = { s : Num => Str } ;
  lincat NP       = { s : Str } ;
  lincat VP       = { s : Str } ;
  lincat V2       = { s : Str } ;
  lincat VPass    = { s : Str } ;
  lincat AP       = { s : Str } ;

  oper noun sg pl = { s = table { Sg => sg ; Pl => pl } } ;

  oper clause subj vp = { s = table {
      Must      => subj ++ "MUST"       ++ vp ;
      MustNot   => subj ++ "MUST NOT"   ++ vp ;
      Should    => subj ++ "SHOULD"     ++ vp ;
      ShouldNot => subj ++ "SHOULD NOT" ++ vp ;
      May       => subj ++ "MAY"        ++ vp
    } } ;

  lin Actor    = { s = [] } ;
  lin Message  = { s = [] } ;
  lin Resource = { s = [] } ;

  lin MUST       = { s = [] ; m = Must } ;
  lin MUST_NOT   = { s = [] ; m = MustNot } ;
  lin SHOULD     = { s = [] ; m = Should } ;
  lin SHOULD_NOT = { s = [] ; m = ShouldNot } ;
  lin MAY        = { s = [] ; m = May } ;

  lin Obligation mo pr = { s = mo.s ++ pr.s ! mo.m } ;

  lin Pred k np vp = clause (k.s ++ np.s) vp.s ;

  lin theDet   = { s = "the"   ; n = Sg } ;
  lin aDet     = { s = "a"     ; n = Sg } ;
  lin anDet    = { s = "an"    ; n = Sg } ;
  lin everyDet = { s = "every" ; n = Sg } ;
  lin allDet   = { s = "all"   ; n = Pl } ;

  lin DetNP k det cn = { s = k.s ++ det.s ++ cn.s ! det.n } ;

  lin OfMod k j cn np = { s = table {
      Sg => k.s ++ j.s ++ cn.s ! Sg ++ "of" ++ np.s ;
      Pl => k.s ++ j.s ++ cn.s ! Pl ++ "of" ++ np.s
    } } ;

  lin cloudPlatform         = noun "cloud platform"         "cloud platforms" ;
  lin provider              = noun "provider"               "providers" ;
  lin user                  = noun "user"                   "users" ;
  lin developer             = noun "developer"              "developers" ;
  lin apiCall               = noun "API call"               "API calls" ;
  lin request               = noun "request"                "requests" ;
  lin indication            = noun "indication"             "indications" ;
  lin credential            = noun "credential"             "credentials" ;
  lin capability            = noun "capability"             "capabilities" ;
  lin specificationTemplate = noun "specification template" "specification templates" ;
  lin account               = noun "account"                "accounts" ;
  lin application           = noun "application"            "applications" ;
  lin endpoint              = noun "endpoint"               "endpoints" ;

  lin determine_V2    = { s = "determine" } ;
  lin configure_V2    = { s = "configure" } ;
  lin store_V2        = { s = "store" } ;
  lin publish_V2      = { s = "publish" } ;
  lin authenticate_V2 = { s = "authenticate" } ;
  lin output_V2       = { s = "output" } ;
  lin redirect_V2     = { s = "redirect" } ;

  lin authenticated_VPass = { s = "authenticated" } ;
  lin stored_VPass        = { s = "stored" } ;
  lin configured_VPass    = { s = "configured" } ;
  lin published_VPass     = { s = "published" } ;

  lin common_AP    = { s = "common" } ;
  lin usable_AP    = { s = "usable" } ;
  lin available_AP = { s = "available" } ;

  lin Act k j v np    = { s = k.s ++ j.s ++ v.s ++ np.s } ;
  lin BePass k v      = { s = k.s ++ "be" ++ v.s } ;
  lin BePassBy k v np = { s = k.s ++ "be" ++ v.s ++ "by" ++ np.s } ;
  lin BeAP k ap       = { s = k.s ++ "be" ++ ap.s } ;
}

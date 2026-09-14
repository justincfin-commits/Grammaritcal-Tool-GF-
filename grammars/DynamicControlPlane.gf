-- The abstract syntax of a requirements fragment for a dynamic control plane.
--
-- The vocabulary comes from the claims of WO 2025/075926, "Dynamic control
-- plane for configuring capabilities across applications via a cloud
-- platform". The claims speak of a cloud platform that determines first
-- information in accordance with a specification template, obtains a
-- credential, redirects a user, configures capabilities for an account of an
-- application, and authenticates API calls to the endpoints of a provider.
--
-- Two things carry the weight here. First, a requirement is indexed by its
-- modal strength: "the cloud platform MUST store the credential" has type
-- Req MUST, and "an account of the application MAY be configured" has type
-- Req MAY. The strength is part of the type, not a note beside it.
--
-- Second, every noun and every verb carries a sort. An Actor does things. A
-- Message travels. A Resource is held and configured. A verb that takes a
-- message will not take an actor, so "the cloud platform MUST authenticate
-- the provider" parses and then fails to type check. The parser cannot tell
-- the sorts apart, because every Sort constructor prints nothing; the type
-- checker can.

abstract DynamicControlPlane = {

  flags startcat = Req ;

  cat Sort ;
  cat Modality ;
  cat Prop ;
  cat Req (m : Modality) ;

  cat Det ;
  cat CN (k : Sort) ;
  cat NP (k : Sort) ;
  cat VP (k : Sort) ;
  cat V2 (k : Sort) (j : Sort) ;
  cat VPass (k : Sort) ;
  cat AP (k : Sort) ;

  -- The three sorts.
  fun Actor, Message, Resource : Sort ;

  -- The five modal strengths of RFC 2119.
  fun MUST, MUST_NOT, SHOULD, SHOULD_NOT, MAY : Modality ;

  -- A requirement is a statement at one modal strength. The result type
  -- mentions the argument, so the strength survives into the type.
  fun Obligation : (m : Modality) -> Prop -> Req m ;

  -- A statement joins a subject to a predicate of the same sort.
  fun Pred : (k : Sort) -> NP k -> VP k -> Prop ;

  -- Determiners. The concrete syntax gives each a number. "a" and "an" are
  -- two constructors because the dialect has no "pre" to choose between them
  -- by the next word; the cost is that "an cloud platform" also parses.
  fun theDet, aDet, anDet, everyDet, allDet : Det ;

  fun DetNP : (k : Sort) -> Det -> CN k -> NP k ;

  -- The "of" modifier. The head keeps its own sort k; the complement carries
  -- its own sort j, so "the developer of the application" is an Actor built
  -- from a Resource.
  fun OfMod : (k : Sort) -> (j : Sort) -> CN k -> NP j -> CN k ;

  -- An active transitive verb takes an object of the sort the verb demands.
  fun Act : (k : Sort) -> (j : Sort) -> V2 k j -> NP j -> VP k ;

  -- A passive verb without an agent, and the same verb with an agent. The
  -- agent is always an Actor.
  fun BePass : (k : Sort) -> VPass k -> VP k ;
  fun BePassBy : (k : Sort) -> VPass k -> NP Actor -> VP k ;

  -- A predicate adjective.
  fun BeAP : (k : Sort) -> AP k -> VP k ;

  -- Nouns, by sort. Several are multiword.
  fun cloudPlatform, provider, user, developer : CN Actor ;
  fun apiCall, request, indication : CN Message ;
  fun credential, capability, specificationTemplate, account, application, endpoint : CN Resource ;

  -- Verbs. The first sort is the subject, the second the object.
  fun determine_V2, configure_V2, store_V2, publish_V2 : V2 Actor Resource ;
  fun authenticate_V2, output_V2 : V2 Actor Message ;
  fun redirect_V2 : V2 Actor Actor ;

  -- Passive participles, by the sort of the thing they are done to.
  fun authenticated_VPass : VPass Message ;
  fun stored_VPass, configured_VPass, published_VPass : VPass Resource ;

  -- Predicate adjectives, by the sort of the thing they describe.
  fun common_AP, usable_AP : AP Resource ;
  fun available_AP : AP Actor ;
}

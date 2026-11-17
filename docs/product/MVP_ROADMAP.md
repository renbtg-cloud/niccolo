# Niccolò MVP roadmap

This document records product requirements that belong to the MVP even when
their detailed architecture is intentionally deferred.

The MVP is not optimized for the shortest possible path to a demo. The goal is
a small but strikingly complete product whose core reasoning, privacy and
collaboration properties are already coherent.

## Current priority — reasoning domain

Before expanding collaboration semantics, finish the core reasoning abstractions
and their invariants. In particular, the domain should remain explicit,
hard-coded, typed and inspectable rather than prematurely generalized into a
plugin/configuration framework.

The reasoning model must be rich enough to represent actors, claims,
observations/evidence, events, relationships, hypotheses and higher-order
relations without flattening competing interpretations into a single narrative.

## MVP requirement — collaborative investigation across Niccolò instances

The MVP **must support collaborative investigation between two or more
independently running Niccolò instances**.

This is not merely multi-user access to one server. Multiple Niccolò
installations must be able to participate in the same broader investigation
while each installation retains its own local state and disclosure authority.

The collaboration design must support **selective disclosure**. A participant
may share only the information that should cross an information boundary rather
than exporting an entire Situation.

This requirement is deliberately architectural, not yet a frozen protocol.

### Required properties

The MVP collaboration design must preserve at least these properties:

- no implicit full-Situation sharing;
- the sending side explicitly controls what is disclosed;
- undisclosed material remains local;
- shared claims, evidence, findings, hypotheses or other graph elements retain
  provenance and revision identity;
- a recipient can distinguish locally-originated material from material received
  from a partner;
- later corrections, withdrawals or superseding disclosures can be represented
  without silently rewriting earlier received state;
- collaboration must remain compatible with local-only / air-gapped operation
  and must not require a central SaaS authority;
- two instances are sufficient for the first implementation, but the domain and
  protocol must not assume that only two participants can ever exist.

### Explicitly deferred design questions

After the reasoning abstractions are sufficiently mature, design the
collaboration/disclosure layer in detail. That later work should address concepts
such as participant identity, information boundaries, disclosure scopes,
compartments / need-to-know, redaction, derived disclosures, shared views,
revocation/withdrawal semantics and multi-party synchronization.

Do **not** prematurely force those concepts into the reasoning model merely to
make collaboration configurable.

## Post-MVP generalization

Do not extract a generic plugin/domain/configuration framework during the MVP
unless a concrete implementation need makes it unavoidable.

The intended sequence is:

1. build a large, explicit, rationalized domain model;
2. validate it through real reasoning workflows;
3. ship the MVP with coherent reasoning, privacy and collaboration;
4. only much later identify abstractions that have actually earned
   configurability or reuse.

Generalization follows understanding, not the reverse.

# MVP collaborative investigation requirement

## Requirement

The MVP must support collaborative work between **two or more independent Niccolò instances** participating in the same investigation.

This is not merely multi-user access to one shared server. The product model must allow separate Niccolò installations/instances to collaborate while retaining their own local state and control over what they disclose.

## Scope for MVP

The MVP requirement is intentionally minimal:

- at least two Niccolò instances can participate in a shared investigation;
- collaboration is not limited to a fixed pair and must not assume exactly two participants;
- each instance remains an independently controlled participant;
- information exchanged between instances must preserve provenance and authorship;
- the design must not require all local investigation state to be shared.

Selective disclosure, compartments, need-to-know rules, derived-disclosure leakage, redaction, shared views and richer collaboration policy will be designed separately after the core reasoning abstractions are settled.

## Architectural constraint

Do not postpone the existence of multi-instance collaboration until after the MVP by designing the MVP around a permanently single-instance assumption.

The first implementation may be deliberately narrow, but the MVP must contain a real collaborative path between independent instances.

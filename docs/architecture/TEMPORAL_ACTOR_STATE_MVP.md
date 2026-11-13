# Qualified temporal actor state

Niccolò records time-scoped assertions about actors without pretending that internal mental state is directly observable.

The MVP can distinguish roles, formal authority, access to information, communications, declared or attributed beliefs, inferred awareness, relationships, decision participation and practical influence. Each assertion carries an epistemic kind and a qualification such as observed, reported, attributed, inferred, uncertain, contradicted or unknown.

`FORMAL_AUTHORITY` and `PRACTICAL_INFLUENCE` are deliberately separate assertion types. Communication volume or hierarchy does not automatically establish either one.

An assertion can point to one frozen source fragment or one frozen human contribution. The API rejects source/contribution references that were not part of the analysis input. Reanalysis creates a new analysis rather than rewriting what an older analysis could have known.

The model-facing reasoning packet receives these qualified records with rules that forbid silently projecting later information backward, treating inferred awareness as observed knowledge, or equating formal authority with practical influence.

The storage boundary enforces the same bounded theory-of-mind rule: an
`INFERRED_AWARENESS` assertion cannot be recorded with `OBSERVED`,
`REPORTED` or `ATTRIBUTED` qualification. It must remain `INFERRED`,
`UNCERTAIN`, `CONTRADICTED` or `UNKNOWN`. What was observed can be stored
as communication/access evidence separately; the leap to another actor's
awareness remains an explicit inference.

This is an instrumental observation-and-inference model. It does not require Niccolò to decide whether an actor has a mind, consciousness or a particular implementation substrate.

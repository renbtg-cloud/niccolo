# Substrate-neutral actors in the MVP

Niccolò can represent an observable actor without requiring that actor to be human or that its internal mechanism be known.

Actor kind is revisable metadata: `UNKNOWN`, `HUMAN`, `SOFTWARE`, `MIXED` or `ORGANIZATION`. It is explicitly not an investigation gate and does not establish a mind, consciousness, intent or credibility.

Source speaker labels can be bound to actors through append-only events. Actor-kind changes and source bindings are frozen by event-number watermarks when an analysis snapshot is created, so later identity corrections do not silently rewrite what an earlier analysis could have seen.

This allows an old Niccolò instance to appear as an actor in prompt/output transcripts exactly as a person can appear in email or chat. Internal configuration or reasoning traces may later add observations, but they are not prerequisites for investigating its observable behavior.

The legacy human identity model remains available for confirmed person mappings. The generic actor registry does not force all existing person-oriented code to be rewritten at once.

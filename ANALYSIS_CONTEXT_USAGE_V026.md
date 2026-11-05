# v0.26 — explicit premise/context usage

Niccolò now records which frozen Situation contributions an analysis actually used. A contribution may be present in the frozen input without being used. Marking it used never upgrades `USER_REPORTED`, `HYPOTHETICAL`, or `INVESTIGATIVE_PREFERENCE` to truth or evidence. The database foreign key prevents a contribution added after the frozen input revision from being retroactively marked as used.

# Integrity scope versus authenticity

Niccolò treats byte/hash continuity, authenticity and completeness as separate questions.

For retained source originals the MVP can recompute SHA-256 over the stored bytes and compare it with the immutable source-document digest. A successful comparison establishes only that the retained bytes match the bytes represented by that stored digest.

It does not establish who created the material before collection, whether upstream content was forged or manipulated, or whether the collected set is complete. Those properties remain separately qualified as unknown, attested, disputed, declared complete, known incomplete or verified only for an explicitly bounded scope.

Assessments are tied to one frozen analysis and document. The reasoning packet receives them with an explicit rule forbidding hash continuity from being promoted into authenticity or completeness.

This directly covers the Aurelius & Finch fixture where an artifact can have a valid manifest while its pre-ingest authenticity remains unknown.

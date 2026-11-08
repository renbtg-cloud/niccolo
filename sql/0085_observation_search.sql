-- Lexical search is a retrieval aid over original source text, never a
-- credibility or truth score. Scope and frozen-revision membership are
-- enforced by application joins and existing RLS.
BEGIN;
CREATE INDEX IF NOT EXISTS evidence_content_simple_fts_idx
  ON niccolo.evidence USING gin (to_tsvector('simple'::regconfig, content));
COMMIT;

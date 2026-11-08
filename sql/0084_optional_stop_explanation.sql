-- MVP human agency: stopping an analysis needs no explanation.
-- Continuing or replanning still requires explicit meaningful human input.
BEGIN;
ALTER TABLE niccolo.agentic_human_responses
  DROP CONSTRAINT IF EXISTS agentic_human_responses_response_text_check;
ALTER TABLE niccolo.agentic_human_responses
  ADD CONSTRAINT agentic_human_responses_response_text_check
  CHECK (length(response_text) <= 4096
         AND (decision = 'STOP' OR btrim(response_text) <> ''));
COMMIT;

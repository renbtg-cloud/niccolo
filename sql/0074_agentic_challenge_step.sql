-- v0.901-v0.925: adversarial challenge is an explicit internal reasoning step.
BEGIN;
ALTER TABLE niccolo.agentic_steps DROP CONSTRAINT agentic_steps_step_kind_check;
ALTER TABLE niccolo.agentic_steps ADD CONSTRAINT agentic_steps_step_kind_check CHECK (step_kind IN (
 'INSPECT_SOURCES','ASSESS_CONTEXT','PROPOSE_CLARIFICATION','SYNTHESIZE','CHALLENGE_SYNTHESIS',
 'CHECK_ASSUMPTIONS','CHECK_ALTERNATIVES','CHECK_STOP_CONDITIONS','REQUEST_HUMAN_DECISION','PREPARE_HANDOFF'));
COMMIT;

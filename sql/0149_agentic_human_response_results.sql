-- Record the result of new human decisions alongside the response.
BEGIN;

ALTER TABLE niccolo.agentic_human_responses
    ADD COLUMN result_operation_state TEXT,
    ADD COLUMN result_plan_revision BIGINT,
    ADD CONSTRAINT agentic_human_response_result_matches_decision CHECK (
        (result_operation_state IS NULL AND result_plan_revision IS NULL)
        OR (
            result_operation_state IS NOT NULL
            AND result_plan_revision IS NOT NULL
            AND CASE decision
                WHEN 'STOP' THEN result_operation_state = 'CANCELLED'
                    AND result_plan_revision = plan_revision
                WHEN 'CONTINUE' THEN result_operation_state = 'RUNNING'
                    AND result_plan_revision = plan_revision
                WHEN 'REPLAN' THEN result_operation_state = 'RUNNING'
                    AND result_plan_revision - 1 = plan_revision
                ELSE false
            END
        )
    );

CREATE UNIQUE INDEX agentic_human_response_result_id
    ON niccolo.agentic_human_responses
        (workspace_id, situation_id, operation_id, response_id)
    WHERE result_plan_revision IS NOT NULL;

COMMIT;

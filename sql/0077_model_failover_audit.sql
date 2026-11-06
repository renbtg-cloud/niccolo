-- v1.201-v1.300: distinguish requested router from provider/model that actually answered.
BEGIN;
ALTER TABLE niccolo.model_invocations ADD COLUMN effective_provider TEXT, ADD COLUMN effective_model TEXT, ADD COLUMN fallback_used BOOLEAN NOT NULL DEFAULT false;
GRANT UPDATE(effective_provider,effective_model,fallback_used) ON niccolo.model_invocations TO niccolo_app;
COMMIT;

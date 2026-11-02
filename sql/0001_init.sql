-- Niccolò POC schema only. Not full Duhme v0180 implementation.
-- All tenant-owned records are workspace-scoped. An authenticated service must
-- set niccolo.workspace_id from trusted server state, not untrusted client input.
BEGIN;
CREATE SCHEMA IF NOT EXISTS niccolo;
CREATE TABLE IF NOT EXISTS niccolo.workspaces (
  workspace_id UUID PRIMARY KEY,
  label TEXT NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE TABLE IF NOT EXISTS niccolo.evidence (
  workspace_id UUID NOT NULL REFERENCES niccolo.workspaces(workspace_id),
  evidence_id UUID NOT NULL,
  source_locator TEXT NOT NULL,
  content TEXT NOT NULL,
  source_class TEXT NOT NULL CHECK (source_class IN ('DIRECT_OBSERVATION','REPORTED_SPEECH','COPY','ANALYST_INFERENCE')),
  admissible BOOLEAN NOT NULL DEFAULT TRUE,
  recorded_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  PRIMARY KEY (workspace_id,evidence_id)
);
CREATE TABLE IF NOT EXISTS niccolo.evidence_derivation (
  workspace_id UUID NOT NULL,
  child_id UUID NOT NULL,
  parent_id UUID NOT NULL,
  relation TEXT NOT NULL CHECK (relation IN ('COPIED_FROM','TRANSCRIBED_FROM','DERIVED_FROM')),
  PRIMARY KEY(workspace_id,child_id,parent_id),
  FOREIGN KEY(workspace_id,child_id) REFERENCES niccolo.evidence(workspace_id,evidence_id),
  FOREIGN KEY(workspace_id,parent_id) REFERENCES niccolo.evidence(workspace_id,evidence_id),
  CHECK(child_id <> parent_id)
);
-- Claims/propositions contain attribution instead of collapsing hearsay into facts.
CREATE TABLE IF NOT EXISTS niccolo.propositions (
  workspace_id UUID NOT NULL REFERENCES niccolo.workspaces(workspace_id),
  proposition_id UUID NOT NULL,
  kind TEXT NOT NULL CHECK(kind IN ('ATOMIC','SAYS','BELIEVES')),
  actor_label TEXT,
  atomic_statement TEXT,
  inner_proposition_id UUID,
  PRIMARY KEY(workspace_id, proposition_id),
  FOREIGN KEY(workspace_id,inner_proposition_id) REFERENCES niccolo.propositions(workspace_id,proposition_id)
    DEFERRABLE INITIALLY IMMEDIATE,
  CHECK ((kind='ATOMIC' AND atomic_statement IS NOT NULL AND inner_proposition_id IS NULL)
      OR (kind IN ('SAYS','BELIEVES') AND actor_label IS NOT NULL AND inner_proposition_id IS NOT NULL AND atomic_statement IS NULL))
);
CREATE TABLE IF NOT EXISTS niccolo.proposition_evidence (
  workspace_id UUID NOT NULL,
  proposition_id UUID NOT NULL,
  evidence_id UUID NOT NULL,
  relation TEXT NOT NULL CHECK (relation IN ('REPORTS','SUPPORTS','CHALLENGES','MENTIONS')),
  PRIMARY KEY(workspace_id,proposition_id,evidence_id,relation),
  FOREIGN KEY(workspace_id,proposition_id) REFERENCES niccolo.propositions(workspace_id,proposition_id),
  FOREIGN KEY(workspace_id,evidence_id) REFERENCES niccolo.evidence(workspace_id,evidence_id)
);
CREATE TABLE IF NOT EXISTS niccolo.investigative_questions (
  workspace_id UUID NOT NULL REFERENCES niccolo.workspaces(workspace_id),
  question_id UUID NOT NULL,
  question TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'PROPOSED' CHECK(status IN ('PROPOSED','INVESTIGATING','RESOLVED','DISMISSED')),
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  PRIMARY KEY(workspace_id,question_id)
);
CREATE INDEX IF NOT EXISTS evidence_by_workspace_time ON niccolo.evidence(workspace_id,recorded_at DESC);
CREATE INDEX IF NOT EXISTS derivation_by_parent ON niccolo.evidence_derivation(workspace_id,parent_id);

DO $$ BEGIN
  IF NOT EXISTS(SELECT 1 FROM pg_roles WHERE rolname='niccolo_app') THEN
    CREATE ROLE niccolo_app NOLOGIN;
  END IF;
END $$;
GRANT USAGE ON SCHEMA niccolo TO niccolo_app;
GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA niccolo TO niccolo_app;

-- RLS also forced for table owners. A dedicated non-superuser app role is
-- required for a real service; SET LOCAL binding is server-owned.
DO $$
DECLARE t TEXT;
BEGIN
  FOREACH t IN ARRAY ARRAY['workspaces','evidence','evidence_derivation','propositions','proposition_evidence','investigative_questions'] LOOP
    EXECUTE format('ALTER TABLE niccolo.%I ENABLE ROW LEVEL SECURITY', t);
    EXECUTE format('ALTER TABLE niccolo.%I FORCE ROW LEVEL SECURITY', t);
    EXECUTE format('CREATE POLICY workspace_filter ON niccolo.%I USING (workspace_id = NULLIF(current_setting(''niccolo.workspace_id'',true),'''')::uuid) WITH CHECK (workspace_id = NULLIF(current_setting(''niccolo.workspace_id'',true),'''')::uuid)',t);
  END LOOP;
END $$;
COMMIT;

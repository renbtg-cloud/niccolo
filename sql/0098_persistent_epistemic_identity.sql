-- Persistent epistemic identity and revisable human-language understanding.
-- This migration is additive: existing analysis-scoped claims/hypotheses remain intact.
BEGIN;

CREATE TABLE niccolo.epistemic_objects (
  workspace_id UUID NOT NULL REFERENCES niccolo.workspaces(workspace_id),
  object_id UUID NOT NULL,
  object_kind TEXT NOT NULL CHECK (object_kind IN ('PROPOSITION','UTTERANCE','UTTERANCE_INTERPRETATION')),
  created_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY (workspace_id,object_id),
  UNIQUE (workspace_id,object_id,object_kind)
);

CREATE TABLE niccolo.epistemic_propositions (
  workspace_id UUID NOT NULL,
  proposition_id UUID NOT NULL,
  object_kind TEXT NOT NULL DEFAULT 'PROPOSITION' CHECK (object_kind='PROPOSITION'),
  statement_text TEXT NOT NULL CHECK (length(statement_text) BETWEEN 1 AND 8192 AND btrim(statement_text) <> ''),
  language TEXT NOT NULL DEFAULT 'en' CHECK (language='en'),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY (workspace_id,proposition_id),
  FOREIGN KEY (workspace_id,proposition_id,object_kind)
    REFERENCES niccolo.epistemic_objects(workspace_id,object_id,object_kind)
);

-- Existing analysis claims remain analysis occurrences/representations. This table maps
-- those local objects onto durable semantic proposition identity without deduplicating
-- occurrence, provenance, credibility or analysis history.
CREATE TABLE niccolo.situation_analysis_claim_propositions (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  analysis_id UUID NOT NULL,
  claim_id UUID NOT NULL,
  proposition_id UUID NOT NULL,
  relation TEXT NOT NULL DEFAULT 'EXPRESSES' CHECK (relation='EXPRESSES'),
  rationale TEXT NOT NULL CHECK (length(rationale) BETWEEN 1 AND 2048 AND btrim(rationale) <> ''),
  recorded_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY (workspace_id,situation_id,analysis_id,claim_id,proposition_id),
  FOREIGN KEY (workspace_id,situation_id,analysis_id,claim_id)
    REFERENCES niccolo.situation_analysis_claims(workspace_id,situation_id,analysis_id,claim_id),
  FOREIGN KEY (workspace_id,proposition_id)
    REFERENCES niccolo.epistemic_propositions(workspace_id,proposition_id)
);
CREATE INDEX situation_analysis_claim_propositions_by_proposition
  ON niccolo.situation_analysis_claim_propositions(workspace_id,proposition_id,situation_id,analysis_id,claim_id);

-- An utterance here is material admitted to the MVP semantic-language boundary.
-- Non-English source artifacts may still exist elsewhere; they simply cannot be entered
-- into this English-only semantic interpretation substrate in MVP.
CREATE TABLE niccolo.epistemic_human_utterances (
  workspace_id UUID NOT NULL,
  utterance_id UUID NOT NULL,
  object_kind TEXT NOT NULL DEFAULT 'UTTERANCE' CHECK (object_kind='UTTERANCE'),
  situation_id UUID,
  utterance_text TEXT NOT NULL CHECK (length(utterance_text) BETWEEN 1 AND 8192 AND btrim(utterance_text) <> ''),
  language TEXT NOT NULL CHECK (language='en'),
  speaker_actor_id UUID,
  source_evidence_id UUID,
  source_contribution_id UUID,
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY (workspace_id,utterance_id),
  FOREIGN KEY (workspace_id,utterance_id,object_kind)
    REFERENCES niccolo.epistemic_objects(workspace_id,object_id,object_kind),
  FOREIGN KEY (workspace_id,situation_id)
    REFERENCES niccolo.situations(workspace_id,situation_id),
  FOREIGN KEY (workspace_id,speaker_actor_id)
    REFERENCES niccolo.actors(workspace_id,actor_id),
  FOREIGN KEY (workspace_id,source_evidence_id)
    REFERENCES niccolo.source_fragments(workspace_id,evidence_id),
  FOREIGN KEY (workspace_id,situation_id,source_contribution_id)
    REFERENCES niccolo.situation_contributions(workspace_id,situation_id,contribution_id),
  CHECK (NOT (source_evidence_id IS NOT NULL AND source_contribution_id IS NOT NULL)),
  CHECK (source_contribution_id IS NULL OR situation_id IS NOT NULL)
);

-- This is deliberately NOT situation_analysis_candidate_interpretations. That existing
-- object means a candidate explanation/hypothesis; this object means current revisable
-- understanding of one human utterance.
CREATE TABLE niccolo.epistemic_utterance_interpretations (
  workspace_id UUID NOT NULL,
  interpretation_id UUID NOT NULL,
  object_kind TEXT NOT NULL DEFAULT 'UTTERANCE_INTERPRETATION' CHECK (object_kind='UTTERANCE_INTERPRETATION'),
  utterance_id UUID NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY (workspace_id,interpretation_id),
  UNIQUE (workspace_id,utterance_id),
  FOREIGN KEY (workspace_id,interpretation_id,object_kind)
    REFERENCES niccolo.epistemic_objects(workspace_id,object_id,object_kind),
  FOREIGN KEY (workspace_id,utterance_id)
    REFERENCES niccolo.epistemic_human_utterances(workspace_id,utterance_id)
);

CREATE TABLE niccolo.epistemic_interpretation_revisions (
  workspace_id UUID NOT NULL,
  interpretation_id UUID NOT NULL,
  revision_id UUID NOT NULL,
  revision_no BIGINT GENERATED ALWAYS AS IDENTITY,
  supersedes_revision_id UUID,
  interpretation_text TEXT NOT NULL CHECK (length(interpretation_text) BETWEEN 1 AND 8192 AND btrim(interpretation_text) <> ''),
  rationale TEXT NOT NULL CHECK (length(rationale) BETWEEN 1 AND 4096 AND btrim(rationale) <> ''),
  producer_kind TEXT NOT NULL CHECK (producer_kind IN ('MODEL','HUMAN','DETERMINISTIC')),
  recorded_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY (workspace_id,interpretation_id,revision_id),
  UNIQUE (workspace_id,revision_no),
  FOREIGN KEY (workspace_id,interpretation_id)
    REFERENCES niccolo.epistemic_utterance_interpretations(workspace_id,interpretation_id),
  FOREIGN KEY (workspace_id,interpretation_id,supersedes_revision_id)
    REFERENCES niccolo.epistemic_interpretation_revisions(workspace_id,interpretation_id,revision_id)
    DEFERRABLE INITIALLY IMMEDIATE
);
CREATE INDEX epistemic_interpretation_revisions_latest
  ON niccolo.epistemic_interpretation_revisions(workspace_id,interpretation_id,revision_no DESC);

CREATE OR REPLACE FUNCTION niccolo.validate_interpretation_revision_linearity() RETURNS trigger
LANGUAGE plpgsql SECURITY INVOKER SET search_path=pg_catalog,niccolo AS $$
DECLARE latest UUID;
BEGIN
  -- Serialize revisions for one interpretation without granting UPDATE on the
  -- immutable parent row. Hash collisions only over-serialize unrelated revisions.
  PERFORM pg_catalog.pg_advisory_xact_lock(
    pg_catalog.hashtextextended(NEW.workspace_id::text || ':' || NEW.interpretation_id::text,0)
  );

  IF EXISTS (
    SELECT 1 FROM niccolo.epistemic_interpretation_revisions r
     WHERE r.workspace_id=NEW.workspace_id
       AND r.interpretation_id=NEW.interpretation_id
       AND r.revision_id=NEW.revision_id
  ) THEN
    RETURN NEW;
  END IF;

  SELECT r.revision_id INTO latest
    FROM niccolo.epistemic_interpretation_revisions r
   WHERE r.workspace_id=NEW.workspace_id AND r.interpretation_id=NEW.interpretation_id
   ORDER BY r.revision_no DESC LIMIT 1;

  IF latest IS NULL THEN
    IF NEW.supersedes_revision_id IS NOT NULL THEN
      RAISE EXCEPTION 'first interpretation revision cannot supersede another revision' USING ERRCODE='23514';
    END IF;
  ELSIF NEW.supersedes_revision_id IS DISTINCT FROM latest THEN
    RAISE EXCEPTION 'interpretation revision must supersede current revision' USING ERRCODE='23514';
  END IF;
  RETURN NEW;
END $$;

CREATE TRIGGER interpretation_revision_linearity
  BEFORE INSERT ON niccolo.epistemic_interpretation_revisions
  FOR EACH ROW EXECUTE FUNCTION niccolo.validate_interpretation_revision_linearity();

CREATE TABLE niccolo.epistemic_interpretation_confirmation_events (
  workspace_id UUID NOT NULL,
  interpretation_id UUID NOT NULL,
  revision_id UUID NOT NULL,
  event_id UUID NOT NULL,
  event_no BIGINT GENERATED ALWAYS AS IDENTITY,
  confirmation_state TEXT NOT NULL CHECK (confirmation_state IN ('CONFIRMED','REJECTED')),
  note TEXT NOT NULL CHECK (length(note) BETWEEN 1 AND 2048 AND btrim(note) <> ''),
  recorded_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY (workspace_id,interpretation_id,revision_id,event_id),
  UNIQUE (workspace_id,event_no),
  FOREIGN KEY (workspace_id,interpretation_id,revision_id)
    REFERENCES niccolo.epistemic_interpretation_revisions(workspace_id,interpretation_id,revision_id)
);
CREATE INDEX epistemic_interpretation_confirmation_latest
  ON niccolo.epistemic_interpretation_confirmation_events(workspace_id,interpretation_id,revision_id,event_no DESC);

-- Bridge current analysis reasoning to the exact meaning revision it relied on. A later
-- interpretation revision can therefore identify downstream claims requiring review.
CREATE TABLE niccolo.situation_analysis_claim_interpretation_dependencies (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  analysis_id UUID NOT NULL,
  claim_id UUID NOT NULL,
  interpretation_id UUID NOT NULL,
  interpretation_revision_id UUID NOT NULL,
  dependency_role TEXT NOT NULL DEFAULT 'DEPENDS_ON_MEANING' CHECK (dependency_role='DEPENDS_ON_MEANING'),
  rationale TEXT NOT NULL CHECK (length(rationale) BETWEEN 1 AND 2048 AND btrim(rationale) <> ''),
  recorded_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY (workspace_id,situation_id,analysis_id,claim_id,interpretation_id,interpretation_revision_id),
  FOREIGN KEY (workspace_id,situation_id,analysis_id,claim_id)
    REFERENCES niccolo.situation_analysis_claims(workspace_id,situation_id,analysis_id,claim_id),
  FOREIGN KEY (workspace_id,interpretation_id,interpretation_revision_id)
    REFERENCES niccolo.epistemic_interpretation_revisions(workspace_id,interpretation_id,revision_id)
);
CREATE INDEX claim_interp_dependencies_by_interpretation
  ON niccolo.situation_analysis_claim_interpretation_dependencies(workspace_id,interpretation_id,interpretation_revision_id,situation_id,analysis_id,claim_id);

GRANT SELECT,INSERT ON
  niccolo.epistemic_objects,
  niccolo.epistemic_propositions,
  niccolo.situation_analysis_claim_propositions,
  niccolo.epistemic_human_utterances,
  niccolo.epistemic_utterance_interpretations,
  niccolo.epistemic_interpretation_revisions,
  niccolo.epistemic_interpretation_confirmation_events,
  niccolo.situation_analysis_claim_interpretation_dependencies
TO niccolo_app;

GRANT USAGE,SELECT ON SEQUENCE
  niccolo.epistemic_interpretation_revisions_revision_no_seq,
  niccolo.epistemic_interpretation_confirmation_events_event_no_seq
TO niccolo_app;

DO $$ DECLARE t TEXT; BEGIN
 FOREACH t IN ARRAY ARRAY[
  'epistemic_objects',
  'epistemic_propositions',
  'situation_analysis_claim_propositions',
  'epistemic_human_utterances',
  'epistemic_utterance_interpretations',
  'epistemic_interpretation_revisions',
  'epistemic_interpretation_confirmation_events',
  'situation_analysis_claim_interpretation_dependencies'
 ] LOOP
  EXECUTE format('ALTER TABLE niccolo.%I ENABLE ROW LEVEL SECURITY',t);
  EXECUTE format('ALTER TABLE niccolo.%I FORCE ROW LEVEL SECURITY',t);
  EXECUTE format('CREATE POLICY workspace_filter ON niccolo.%I USING (workspace_id = NULLIF(current_setting(''niccolo.workspace_id'',true),'''')::uuid) WITH CHECK (workspace_id = NULLIF(current_setting(''niccolo.workspace_id'',true),'''')::uuid)',t);
  EXECUTE format('CREATE POLICY api_read_gate ON niccolo.%I AS RESTRICTIVE FOR SELECT TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,''read''))',t);
  EXECUTE format('CREATE POLICY api_write_insert_gate ON niccolo.%I AS RESTRICTIVE FOR INSERT TO niccolo_api WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,''write''))',t);
  EXECUTE format('CREATE POLICY api_write_update_gate ON niccolo.%I AS RESTRICTIVE FOR UPDATE TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,''write'')) WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,''write''))',t);
  EXECUTE format('CREATE POLICY api_write_delete_gate ON niccolo.%I AS RESTRICTIVE FOR DELETE TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,''write''))',t);
 END LOOP;
END $$;

COMMIT;

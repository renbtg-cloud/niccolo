-- Expert-authored epistemic workbench substrate.
-- MVP users explicitly author structured abstractions; investigator free-text parsing is post-MVP.
BEGIN;

ALTER TABLE niccolo.epistemic_objects
  DROP CONSTRAINT epistemic_objects_object_kind_check;
ALTER TABLE niccolo.epistemic_objects
  ADD CONSTRAINT epistemic_objects_object_kind_check CHECK (
    object_kind IN (
      'PROPOSITION','HYPOTHESIS','ASSUMPTION','INFORMATION_GAP','FINDING','ARGUMENT',
      'UTTERANCE','UTTERANCE_INTERPRETATION'
    )
  );

CREATE TABLE niccolo.epistemic_hypotheses (
  workspace_id UUID NOT NULL,
  hypothesis_id UUID NOT NULL,
  object_kind TEXT NOT NULL DEFAULT 'HYPOTHESIS' CHECK (object_kind='HYPOTHESIS'),
  situation_id UUID NOT NULL,
  statement_text TEXT NOT NULL CHECK (length(statement_text) BETWEEN 1 AND 8192 AND btrim(statement_text)<>''),
  rationale TEXT NOT NULL CHECK (length(rationale) BETWEEN 1 AND 4096 AND btrim(rationale)<>''),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY (workspace_id,hypothesis_id),
  FOREIGN KEY (workspace_id,hypothesis_id,object_kind)
    REFERENCES niccolo.epistemic_objects(workspace_id,object_id,object_kind),
  FOREIGN KEY (workspace_id,situation_id)
    REFERENCES niccolo.situations(workspace_id,situation_id)
);
CREATE INDEX epistemic_hypotheses_by_situation
  ON niccolo.epistemic_hypotheses(workspace_id,situation_id,created_at,hypothesis_id);

CREATE TABLE niccolo.epistemic_assumptions (
  workspace_id UUID NOT NULL,
  assumption_id UUID NOT NULL,
  object_kind TEXT NOT NULL DEFAULT 'ASSUMPTION' CHECK (object_kind='ASSUMPTION'),
  situation_id UUID NOT NULL,
  statement_text TEXT NOT NULL CHECK (length(statement_text) BETWEEN 1 AND 8192 AND btrim(statement_text)<>''),
  rationale TEXT NOT NULL CHECK (length(rationale) BETWEEN 1 AND 4096 AND btrim(rationale)<>''),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY (workspace_id,assumption_id),
  FOREIGN KEY (workspace_id,assumption_id,object_kind)
    REFERENCES niccolo.epistemic_objects(workspace_id,object_id,object_kind),
  FOREIGN KEY (workspace_id,situation_id)
    REFERENCES niccolo.situations(workspace_id,situation_id)
);
CREATE INDEX epistemic_assumptions_by_situation
  ON niccolo.epistemic_assumptions(workspace_id,situation_id,created_at,assumption_id);

CREATE TABLE niccolo.epistemic_information_gaps (
  workspace_id UUID NOT NULL,
  gap_id UUID NOT NULL,
  object_kind TEXT NOT NULL DEFAULT 'INFORMATION_GAP' CHECK (object_kind='INFORMATION_GAP'),
  situation_id UUID NOT NULL,
  description TEXT NOT NULL CHECK (length(description) BETWEEN 1 AND 8192 AND btrim(description)<>''),
  rationale TEXT NOT NULL CHECK (length(rationale) BETWEEN 1 AND 4096 AND btrim(rationale)<>''),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY (workspace_id,gap_id),
  FOREIGN KEY (workspace_id,gap_id,object_kind)
    REFERENCES niccolo.epistemic_objects(workspace_id,object_id,object_kind),
  FOREIGN KEY (workspace_id,situation_id)
    REFERENCES niccolo.situations(workspace_id,situation_id)
);
CREATE INDEX epistemic_information_gaps_by_situation
  ON niccolo.epistemic_information_gaps(workspace_id,situation_id,created_at,gap_id);

CREATE TABLE niccolo.epistemic_findings (
  workspace_id UUID NOT NULL,
  finding_id UUID NOT NULL,
  object_kind TEXT NOT NULL DEFAULT 'FINDING' CHECK (object_kind='FINDING'),
  situation_id UUID NOT NULL,
  statement_text TEXT NOT NULL CHECK (length(statement_text) BETWEEN 1 AND 8192 AND btrim(statement_text)<>''),
  rationale TEXT NOT NULL CHECK (length(rationale) BETWEEN 1 AND 4096 AND btrim(rationale)<>''),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY (workspace_id,finding_id),
  FOREIGN KEY (workspace_id,finding_id,object_kind)
    REFERENCES niccolo.epistemic_objects(workspace_id,object_id,object_kind),
  FOREIGN KEY (workspace_id,situation_id)
    REFERENCES niccolo.situations(workspace_id,situation_id)
);
CREATE INDEX epistemic_findings_by_situation
  ON niccolo.epistemic_findings(workspace_id,situation_id,created_at,finding_id);

CREATE TABLE niccolo.epistemic_arguments (
  workspace_id UUID NOT NULL,
  argument_id UUID NOT NULL,
  object_kind TEXT NOT NULL DEFAULT 'ARGUMENT' CHECK (object_kind='ARGUMENT'),
  situation_id UUID NOT NULL,
  conclusion_object_id UUID NOT NULL,
  argument_mode TEXT NOT NULL CHECK (argument_mode IN ('DEDUCTIVE','INDUCTIVE','ABDUCTIVE','CAUSAL','ANALOGICAL','POLICY','OTHER')),
  rationale TEXT NOT NULL CHECK (length(rationale) BETWEEN 1 AND 4096 AND btrim(rationale)<>''),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY (workspace_id,argument_id),
  FOREIGN KEY (workspace_id,argument_id,object_kind)
    REFERENCES niccolo.epistemic_objects(workspace_id,object_id,object_kind),
  FOREIGN KEY (workspace_id,situation_id)
    REFERENCES niccolo.situations(workspace_id,situation_id),
  FOREIGN KEY (workspace_id,conclusion_object_id)
    REFERENCES niccolo.epistemic_objects(workspace_id,object_id),
  CHECK (argument_id <> conclusion_object_id)
);
CREATE INDEX epistemic_arguments_by_situation
  ON niccolo.epistemic_arguments(workspace_id,situation_id,created_at,argument_id);
CREATE INDEX epistemic_arguments_by_conclusion
  ON niccolo.epistemic_arguments(workspace_id,conclusion_object_id,argument_id);

CREATE TABLE niccolo.epistemic_relations (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  relation_id UUID NOT NULL,
  relation_family TEXT NOT NULL CHECK (relation_family IN ('SEMANTIC','GENEALOGY','CONTEXTUAL','CAUSAL')),
  relation_kind TEXT NOT NULL,
  left_object_id UUID NOT NULL,
  right_object_id UUID NOT NULL,
  rationale TEXT NOT NULL CHECK (length(rationale) BETWEEN 1 AND 4096 AND btrim(rationale)<>''),
  recorded_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY (workspace_id,relation_id),
  FOREIGN KEY (workspace_id,situation_id)
    REFERENCES niccolo.situations(workspace_id,situation_id),
  FOREIGN KEY (workspace_id,left_object_id)
    REFERENCES niccolo.epistemic_objects(workspace_id,object_id),
  FOREIGN KEY (workspace_id,right_object_id)
    REFERENCES niccolo.epistemic_objects(workspace_id,object_id),
  CHECK (left_object_id <> right_object_id),
  CHECK (
    (relation_family='SEMANTIC' AND relation_kind IN ('EQUIVALENT_TO','MORE_SPECIFIC_THAN','ENTAILS','CONTRADICTS','PARTIAL_OVERLAP')) OR
    (relation_family='GENEALOGY' AND relation_kind IN ('REFINES','SUPERSEDES','RETRACTS','CONSOLIDATES','SEPARATES','REINTERPRETS')) OR
    (relation_family='CONTEXTUAL' AND relation_kind IN ('RELEVANT_TO','APPLICABLE_UNDER')) OR
    (relation_family='CAUSAL' AND relation_kind IN ('CAUSES','CONTRIBUTES_TO','ENABLES','PREVENTS','INTENSIFIES','ATTENUATES'))
  )
);
CREATE INDEX epistemic_relations_by_situation
  ON niccolo.epistemic_relations(workspace_id,situation_id,relation_family,relation_kind,created_at,relation_id);
CREATE INDEX epistemic_relations_by_left
  ON niccolo.epistemic_relations(workspace_id,left_object_id,relation_kind,relation_id);
CREATE INDEX epistemic_relations_by_right
  ON niccolo.epistemic_relations(workspace_id,right_object_id,relation_kind,relation_id);
CREATE UNIQUE INDEX epistemic_relations_symmetric_unique
  ON niccolo.epistemic_relations(
    workspace_id,situation_id,relation_kind,
    LEAST(left_object_id,right_object_id),GREATEST(left_object_id,right_object_id)
  )
  WHERE relation_kind IN ('EQUIVALENT_TO','CONTRADICTS','PARTIAL_OVERLAP');

-- One grounding set describes one joint route from its members toward an argument's
-- conclusion. Multiple sets under an argument remain distinct alternative routes.
CREATE TABLE niccolo.epistemic_grounding_sets (
  workspace_id UUID NOT NULL,
  argument_id UUID NOT NULL,
  grounding_set_id UUID NOT NULL,
  effect TEXT NOT NULL CHECK (effect IN ('SUPPORTS','WEAKENS','CHALLENGES','CONDITIONS')),
  adequacy TEXT NOT NULL CHECK (adequacy IN ('UNSPECIFIED','PARTIAL','SUFFICIENT','NECESSARY_CONDITION')),
  rationale TEXT NOT NULL CHECK (length(rationale) BETWEEN 1 AND 4096 AND btrim(rationale)<>''),
  recorded_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY (workspace_id,argument_id,grounding_set_id),
  FOREIGN KEY (workspace_id,argument_id)
    REFERENCES niccolo.epistemic_arguments(workspace_id,argument_id)
);
CREATE INDEX epistemic_grounding_sets_by_argument
  ON niccolo.epistemic_grounding_sets(workspace_id,argument_id,created_at,grounding_set_id);

CREATE TABLE niccolo.epistemic_grounding_members (
  workspace_id UUID NOT NULL,
  argument_id UUID NOT NULL,
  grounding_set_id UUID NOT NULL,
  member_id UUID NOT NULL,
  member_role TEXT NOT NULL CHECK (member_role IN ('PREMISE','ASSUMPTION','EVIDENCE','CONTEXT')),
  ground_object_id UUID,
  evidence_id UUID,
  rationale TEXT NOT NULL CHECK (length(rationale) BETWEEN 1 AND 2048 AND btrim(rationale)<>''),
  recorded_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY (workspace_id,argument_id,grounding_set_id,member_id),
  FOREIGN KEY (workspace_id,argument_id,grounding_set_id)
    REFERENCES niccolo.epistemic_grounding_sets(workspace_id,argument_id,grounding_set_id),
  FOREIGN KEY (workspace_id,ground_object_id)
    REFERENCES niccolo.epistemic_objects(workspace_id,object_id),
  FOREIGN KEY (workspace_id,evidence_id)
    REFERENCES niccolo.source_fragments(workspace_id,evidence_id),
  CHECK ((ground_object_id IS NULL) <> (evidence_id IS NULL)),
  CHECK ((evidence_id IS NOT NULL AND member_role='EVIDENCE') OR
         (ground_object_id IS NOT NULL AND member_role<>'EVIDENCE'))
);
CREATE INDEX epistemic_grounding_members_by_object
  ON niccolo.epistemic_grounding_members(workspace_id,ground_object_id,argument_id,grounding_set_id)
  WHERE ground_object_id IS NOT NULL;
CREATE INDEX epistemic_grounding_members_by_evidence
  ON niccolo.epistemic_grounding_members(workspace_id,evidence_id,argument_id,grounding_set_id)
  WHERE evidence_id IS NOT NULL;

GRANT SELECT,INSERT ON
  niccolo.epistemic_hypotheses,
  niccolo.epistemic_assumptions,
  niccolo.epistemic_information_gaps,
  niccolo.epistemic_findings,
  niccolo.epistemic_arguments,
  niccolo.epistemic_relations,
  niccolo.epistemic_grounding_sets,
  niccolo.epistemic_grounding_members
TO niccolo_app;

DO $$ DECLARE t TEXT; BEGIN
 FOREACH t IN ARRAY ARRAY[
  'epistemic_hypotheses',
  'epistemic_assumptions',
  'epistemic_information_gaps',
  'epistemic_findings',
  'epistemic_arguments',
  'epistemic_relations',
  'epistemic_grounding_sets',
  'epistemic_grounding_members'
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

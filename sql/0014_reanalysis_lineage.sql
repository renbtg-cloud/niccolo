-- v0.20: explicit lineage between a reanalysis and the earlier analysis it revisits.
BEGIN;
ALTER TABLE niccolo.situation_analysis_snapshots
  ADD COLUMN reanalysis_of UUID;

ALTER TABLE niccolo.situation_analysis_snapshots
  ADD CONSTRAINT situation_analysis_snapshots_reanalysis_fk
  FOREIGN KEY (workspace_id, situation_id, reanalysis_of)
  REFERENCES niccolo.situation_analysis_snapshots(workspace_id, situation_id, analysis_id),
  ADD CONSTRAINT situation_analysis_snapshots_not_self_reanalysis
  CHECK (reanalysis_of IS NULL OR reanalysis_of <> analysis_id);
COMMIT;

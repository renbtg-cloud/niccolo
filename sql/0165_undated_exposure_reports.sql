BEGIN;

ALTER TABLE niccolo.public_prominence_records
  ALTER COLUMN effective_from DROP NOT NULL,
  ALTER COLUMN effective_through DROP NOT NULL,
  ADD CONSTRAINT prominence_event_dates CHECK (
    (effective_from IS NOT NULL AND effective_through IS NOT NULL)
    OR (effective_from IS NULL AND effective_through IS NULL AND observation = 'EXPOSURE_REPORTED')
  );

COMMIT;

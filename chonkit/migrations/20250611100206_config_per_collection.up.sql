-- Add collection_id to parsers

ALTER TABLE parsers ADD COLUMN collection_id UUID REFERENCES collections ON DELETE CASCADE;

CREATE INDEX ON parsers (collection_id);

DO $$
DECLARE
  constraint_name text;
BEGIN
  SELECT conname INTO constraint_name FROM pg_constraint
  WHERE conrelid = 'parsers'::regclass
    AND conname LIKE '%document_id%'
    AND contype = 'u';

  IF constraint_name IS NOT NULL THEN
    EXECUTE format('ALTER TABLE parsers DROP CONSTRAINT %I', constraint_name);
  END IF;
END $$;

ALTER TABLE parsers ADD CONSTRAINT unique_parser UNIQUE (document_id, collection_id);


-- Add collection_id to chunkers

ALTER TABLE chunkers ADD COLUMN collection_id UUID REFERENCES collections ON DELETE CASCADE;

DO $$
DECLARE
  constraint_name text;
BEGIN
  SELECT conname INTO constraint_name FROM pg_constraint
  WHERE conrelid = 'chunkers'::regclass
    AND conname LIKE '%document_id%'
    AND contype = 'u';

  IF constraint_name IS NOT NULL THEN
    EXECUTE format('ALTER TABLE chunkers DROP CONSTRAINT %I', constraint_name);
  END IF;
END $$;

CREATE INDEX ON chunkers (collection_id);

ALTER TABLE chunkers ADD CONSTRAINT unique_chunker UNIQUE (document_id, collection_id);

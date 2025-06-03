CREATE TABLE images(
    path TEXT PRIMARY KEY,
    document_id UUID NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    src TEXT NOT NULL,
    description TEXT
);

CREATE INDEX ON images(document_id);
CREATE INDEX ON images(src);

ALTER TABLE embedding_reports ADD COLUMN image_vectors INTEGER;

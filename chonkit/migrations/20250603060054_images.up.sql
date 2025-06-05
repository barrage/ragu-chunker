CREATE TABLE images(
    -- Path is obtained based on document hash
    path TEXT PRIMARY KEY,
    -- Document realted metadata
    page_number INTEGER NOT NULL,
    image_number INTEGER NOT NULL,
    format TEXT NOT NULL,
    hash TEXT NOT NULL,
    document_id UUID NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    src TEXT NOT NULL,
    width INTEGER NOT NULL,
    height INTEGER NOT NULL,
    description TEXT
);

CREATE INDEX ON images(document_id);
CREATE INDEX ON images(src);

ALTER TABLE embedding_reports ADD COLUMN image_vectors INTEGER;

CREATE TABLE embedding_reports (
    id SERIAL PRIMARY KEY,
    -- Text, image, etc.
    type TEXT NOT NULL,
    collection_id UUID REFERENCES collections ON DELETE SET NULL,
    collection_name TEXT NOT NULL,
    vector_db TEXT NOT NULL,
    started_at TIMESTAMPTZ NOT NULL,
    finished_at TIMESTAMPTZ NOT NULL,

    model_used TEXT NOT NULL,
    embedding_provider TEXT NOT NULL,
    total_vectors INTEGER NOT NULL,
    tokens_used INTEGER,
    cache BOOLEAN NOT NULL,

    -- The following fields will depend on type

    document_id UUID REFERENCES documents ON DELETE SET NULL,
    document_name TEXT,

    image_id UUID REFERENCES images ON DELETE SET NULL
);

CREATE TABLE embedding_removal_reports(
    id SERIAL PRIMARY KEY,
    -- Text, image, etc.
    type TEXT NOT NULL,
    collection_id UUID REFERENCES collections ON DELETE SET NULL,
    collection_name TEXT NOT NULL,
    vector_db TEXT NOT NULL,
    started_at TIMESTAMPTZ NOT NULL,
    finished_at TIMESTAMPTZ NOT NULL,

    -- The following fields will depend on type

    document_id UUID REFERENCES documents ON DELETE SET NULL,
    document_name TEXT,

    image_id UUID REFERENCES images ON DELETE SET NULL
);

CREATE INDEX ON embedding_reports (type);
CREATE INDEX ON embedding_reports (collection_id);
CREATE INDEX ON embedding_reports (vector_db);
CREATE INDEX ON embedding_reports (model_used);
CREATE INDEX ON embedding_reports (embedding_provider);
CREATE INDEX ON embedding_reports (document_id);
CREATE INDEX ON embedding_reports (document_name);
CREATE INDEX ON embedding_reports (image_id);

CREATE INDEX ON embedding_removal_reports (type);
CREATE INDEX ON embedding_removal_reports (collection_id);
CREATE INDEX ON embedding_removal_reports (vector_db);
CREATE INDEX ON embedding_removal_reports (document_id);
CREATE INDEX ON embedding_removal_reports (document_name);
CREATE INDEX ON embedding_removal_reports (image_id);


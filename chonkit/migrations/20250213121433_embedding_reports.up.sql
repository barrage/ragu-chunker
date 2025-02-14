CREATE TABLE embedding_reports (
    id SERIAL PRIMARY KEY,
    collection_id UUID REFERENCES collections ON DELETE SET NULL,
    collection_name TEXT NOT NULL,
    document_id UUID REFERENCES documents ON DELETE SET NULL,
    document_name TEXT NOT NULL,
    model_used TEXT NOT NULL,
    embedding_provider TEXT NOT NULL,
    vector_db TEXT NOT NULL,
    total_vectors INTEGER NOT NULL,
    tokens_used INTEGER,
    cache BOOLEAN NOT NULL,
    started_at TIMESTAMPTZ NOT NULL,
    finished_at TIMESTAMPTZ NOT NULL
);

CREATE TABLE embedding_removal_reports(
    id SERIAL PRIMARY KEY,
    document_id UUID REFERENCES documents ON DELETE SET NULL,
    document_name TEXT NOT NULL,
    collection_id UUID REFERENCES collections ON DELETE SET NULL,
    collection_name TEXT NOT NULL,
    started_at TIMESTAMPTZ NOT NULL,
    finished_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX embedding_reports_collection_id_idx ON embedding_reports (collection_id);
CREATE INDEX embedding_reports_document_id_idx ON embedding_reports (document_id);

CREATE INDEX embedding_removal_reports_document_id_idx ON embedding_removal_reports (document_id);
CREATE INDEX embedding_removal_reports_collection_id_idx ON embedding_removal_reports (collection_id);

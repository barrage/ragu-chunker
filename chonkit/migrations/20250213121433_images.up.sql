CREATE TABLE images(
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    path TEXT NOT NULL,
    format TEXT NOT NULL,
    hash TEXT NOT NULL,
    src TEXT NOT NULL,
    width INTEGER NOT NULL,
    height INTEGER NOT NULL,
    description TEXT,

    -- Document related metadata
    document_id UUID REFERENCES documents(id) ON DELETE CASCADE,
    page_number INTEGER,
    image_number INTEGER,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX ON images(path);
CREATE INDEX ON images(hash);
CREATE INDEX ON images(src);
CREATE INDEX ON images(document_id);

CREATE TABLE image_embeddings(
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    image_id UUID NOT NULL REFERENCES images(id) ON DELETE CASCADE,
    collection_id UUID NOT NULL REFERENCES collections(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX ON image_embeddings(image_id);
CREATE INDEX ON image_embeddings(collection_id);


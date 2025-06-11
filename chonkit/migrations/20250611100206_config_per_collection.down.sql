ALTER TABLE parsers DROP COLUMN collection_id;
ALTER TABLE chunkers DROP COLUMN collection_id;
ALTER TABLE parsers ADD CONSTRAINT parsers_unique_document_id UNIQUE(document_id);
ALTER TABLE chunkers ADD CONSTRAINT chunkers_unique_document_id UNIQUE(document_id);

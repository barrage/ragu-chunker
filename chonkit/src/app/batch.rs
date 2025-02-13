use crate::{
    core::{
        model::embedding::{
            EmbeddingRemovalReportBuilder, EmbeddingRemovalReportInsert, EmbeddingReportBuilder,
            EmbeddingReportInsert,
        },
        service::{embedding::CreateEmbeddings, ServiceState},
    },
    err,
    error::ChonkitError,
};
use chrono::Utc;
use serde::Serialize;
use std::collections::HashMap;
use tokio::{select, sync::mpsc};
use uuid::Uuid;

/// Sending end for batch embedding jobs.
pub type BatchEmbedderHandle = mpsc::Sender<BatchJob>;

pub fn start_batch_embedder(state: ServiceState) -> BatchEmbedderHandle {
    let (tx, rx) = mpsc::channel(128);
    BatchEmbedder::new(rx, state).start();
    tx
}

pub struct BatchEmbedder {
    /// Job queue.
    q: HashMap<Uuid, mpsc::Sender<JobResult>>,

    /// Job receiver.
    job_rx: mpsc::Receiver<BatchJob>,

    /// Report receiver. Receives an embedding report every time
    /// a document is embedded.
    result_rx: mpsc::Receiver<BatchJobResult>,

    /// Use as the sending end of the report channel.
    /// Here solely so we can just clone it for jobs.
    result_tx: mpsc::Sender<BatchJobResult>,

    state: ServiceState,
}

impl BatchEmbedder {
    pub fn new(job_rx: mpsc::Receiver<BatchJob>, state: ServiceState) -> Self {
        let (result_tx, result_rx) = mpsc::channel(128);
        Self {
            q: HashMap::new(),
            job_rx,
            state,
            result_tx,
            result_rx,
        }
    }

    pub fn start(mut self) {
        tokio::spawn(async move {
            loop {
                select! {
                    job = self.job_rx.recv() => {
                        let Some(job) = job else {
                            tracing::info!("Job receiver channel closed, shutting down executor");
                            break;
                        };

                        let job_id = Uuid::new_v4();
                        let state = self.state.clone();
                        let result_tx = self.result_tx.clone();

                        let BatchJob { collection, add, remove, finished_tx } = job;

                        self.q.insert(job_id, finished_tx);

                        tracing::info!("Starting job '{job_id}' | Adding {} | Removing {}", add.len(), remove.len());

                        tokio::spawn(
                            Self::execute_job(job_id, state, add, remove, collection, result_tx)
                        );
                    }

                    result = self.result_rx.recv() => {
                        let Some(result) = result else {
                            tracing::warn!("Job result channel closed, shutting down executor");
                            break;
                        };

                        let result = match result {
                            BatchJobResult::Event(result) => result,
                            BatchJobResult::Done(id) => {
                                self.q.remove(&id);
                                tracing::debug!("Job '{id}' finished, removing from queue");
                                continue;
                            }
                        };

                        let JobEvent { job_id, result } = result;

                        let Some(finished_tx) = self.q.get(&job_id) else {
                            continue;
                        };

                        let result = finished_tx.send(result).await;

                        tracing::debug!("Sent result to channel ({result:?})");
                    }
                }
            }
        });
    }

    async fn execute_job(
        job_id: Uuid,
        services: ServiceState,
        add: Vec<Uuid>,
        remove: Vec<Uuid>,
        collection_id: Uuid,
        result_tx: mpsc::Sender<BatchJobResult>,
    ) {
        /// Matches the result and continues on error, sending the error to the result channel.
        macro_rules! ok_or_continue {
            ($e:expr) => {
                match $e {
                    Ok(v) => v,
                    Err(e) => {
                        tracing::debug!("Sending error to channel ({:?})", e.error);
                        e.print();
                        let result = JobEvent {
                            job_id,
                            result: JobResult::Err(e),
                        };
                        let _ = result_tx.send(BatchJobResult::Event(result)).await;
                        continue;
                    }
                }
            };
        }

        /// Matches the result and returns on error, sending the error to the result channel.
        macro_rules! ok_or_return {
            ($e:expr) => {
                match $e {
                    Ok(v) => v,
                    Err(e) => {
                        tracing::debug!("Sending error to channel ({e:?})");
                        let result = JobEvent {
                            job_id,
                            result: JobResult::Err(e),
                        };
                        let _ = result_tx.send(BatchJobResult::Event(result)).await;
                        let _ = result_tx.send(BatchJobResult::Done(job_id)).await;
                        return;
                    }
                }
            };
        }

        let collection = ok_or_return!(services.collection.get_collection(collection_id).await);

        for document_id in add.into_iter() {
            tracing::debug!("Processing document '{document_id}'");

            // Map the existence of the embeddings as an error
            let embeddings = ok_or_continue!(
                services
                    .embedding
                    .get_embeddings(document_id, collection_id)
                    .await
            );

            let exists = if embeddings.is_some() {
                err!(
                    AlreadyExists,
                    "Embeddings for '{document_id}' in collection '{collection_id}'"
                )
            } else {
                Ok(())
            };

            ok_or_continue!(exists);

            let document = ok_or_continue!(services.document.get_document(document_id).await);

            // Initialize the report so we get the timestamp before the embedding starts
            let report = EmbeddingReportBuilder::new(
                document.id,
                document.name.clone(),
                collection.id,
                collection.name.clone(),
            );

            // Get the content and chunk it
            let content = ok_or_continue!(services.document.get_content(document.id).await);
            let chunks = ok_or_continue!(services.document.get_chunks(&document, &content).await);
            let chunks = match chunks {
                crate::core::chunk::ChunkedDocument::Ref(r) => r,
                crate::core::chunk::ChunkedDocument::Owned(ref o) => {
                    o.iter().map(|s| s.as_str()).collect()
                }
            };

            let create = CreateEmbeddings {
                document_id: document.id,
                collection_id: collection.id,
                chunks: &chunks,
            };

            let embeddings = ok_or_continue!(services.embedding.create_embeddings(create).await);

            let report = report
                .model_used(collection.model.clone())
                .vector_db(collection.provider.clone())
                .total_vectors(chunks.len())
                .tokens_used(embeddings.tokens_used)
                .embedding_provider(collection.embedder.clone())
                .finished_at(Utc::now())
                .build();

            ok_or_continue!(services.embedding.store_embedding_report(&report).await);

            let result = JobEvent {
                job_id,
                result: JobResult::Ok(JobReport::Addition(report)),
            };

            let _ = result_tx.send(BatchJobResult::Event(result)).await;
        }

        for document_id in remove.into_iter() {
            let document = ok_or_continue!(services.document.get_document(document_id).await);

            let report = EmbeddingRemovalReportBuilder::new(
                document.id,
                document.name.clone(),
                collection.id,
                collection.name.clone(),
            );

            let (_, total_deleted) = ok_or_continue!(
                services
                    .embedding
                    .delete_embeddings(collection.id, document.id)
                    .await
            );

            let report = report
                .total_vectors_removed(total_deleted)
                .finished_at(Utc::now())
                .build();

            ok_or_continue!(
                services
                    .embedding
                    .store_embedding_removal_report(&report)
                    .await
            );

            let result = JobEvent {
                job_id,
                result: JobResult::Ok(JobReport::Removal(report)),
            };

            let _ = result_tx.send(BatchJobResult::Event(result)).await;
        }

        let _ = result_tx.send(BatchJobResult::Done(job_id)).await;
    }
}

pub type JobResult = Result<JobReport, ChonkitError>;

/// Used for batch embedding jobs.
#[derive(Debug)]
pub struct BatchJob {
    /// Collection ID, i.e. where to store the embeddings.
    collection: Uuid,

    /// Documents to embed and add to the collection.
    add: Vec<Uuid>,

    /// Documents to remove from the collection.
    remove: Vec<Uuid>,

    /// Sends finished document embeddings back to whatever sent the job.
    finished_tx: mpsc::Sender<JobResult>,
}

impl BatchJob {
    pub fn new(
        collection: Uuid,
        add: Vec<Uuid>,
        remove: Vec<Uuid>,
        finished_tx: mpsc::Sender<JobResult>,
    ) -> Self {
        Self {
            collection,
            add,
            remove,
            finished_tx,
        }
    }
}

/// Used internally to track the status of an embedding job.
/// If a `Done` is received, the job is removed from the executor's queue.
#[derive(Debug)]
enum BatchJobResult {
    /// Represents a finished removal or addition job with respect to the document.
    Event(JobEvent),

    /// Represents a completely finished job.
    Done(Uuid),
}

/// Represents a single document embedding result in a job.
#[derive(Debug)]
struct JobEvent {
    /// ID of the job the embedding happened in.
    job_id: Uuid,

    /// Result of the embedding process.
    result: Result<JobReport, ChonkitError>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum JobReport {
    /// Job type for adding documents to a collection.
    Addition(EmbeddingReportInsert),

    /// Job type for removing documents from a collection.
    Removal(EmbeddingRemovalReportInsert),
}

use crate::{
    core::{
        model::embedding::{EmbeddingReportAddition, EmbeddingReportRemoval},
        service::{embedding::EmbedSingleInput, ServiceState},
    },
    error::ChonkitError,
};
use serde::Serialize;
use std::collections::HashMap;
use tokio::{select, sync::mpsc};
use uuid::Uuid;

/// Sending end for batch embedding jobs.
pub type BatchEmbedderHandle = mpsc::Sender<BatchJob>;

pub fn start_batch_embedder(state: ServiceState<deadpool_redis::Pool>) -> BatchEmbedderHandle {
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

    state: ServiceState<deadpool_redis::Pool>,
}

impl BatchEmbedder {
    pub fn new(
        job_rx: mpsc::Receiver<BatchJob>,
        state: ServiceState<deadpool_redis::Pool>,
    ) -> Self {
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
        services: ServiceState<deadpool_redis::Pool>,
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

        let total = add.len();
        let collection = match services.collection.get_collection(collection_id).await {
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
        };

        for (i, document_id) in add.into_iter().enumerate() {
            tracing::debug!("Processing document '{document_id}' ({}/{total})", i + 1);

            let report = ok_or_continue!(
                services
                    .embedding
                    .create_embeddings(EmbedSingleInput::new(document_id, collection.id))
                    .await
            );

            let result = JobEvent {
                job_id,
                result: JobResult::Ok(JobReport::Addition(report)),
            };

            let _ = result_tx.send(BatchJobResult::Event(result)).await;
        }

        for document_id in remove.into_iter() {
            let report = ok_or_continue!(
                services
                    .embedding
                    .delete_embeddings(collection.id, document_id)
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
    Addition(EmbeddingReportAddition),

    /// Job type for removing documents from a collection.
    Removal(EmbeddingReportRemoval),
}

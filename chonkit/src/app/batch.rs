use crate::{
    core::{
        model::embedding::EmbeddingReportType,
        service::{embedding::EmbedTextInput, ServiceState},
    },
    error::ChonkitError,
};
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
    /// Maps job IDs to the sending end of the job result channel.
    q: HashMap<Uuid, mpsc::Sender<BatchJobResult>>,

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

                        let BatchJob { collection, add, remove, result_tx: job_result_tx } = job;

                        self.q.insert(job_id, job_result_tx);

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

                         match result {
                            BatchJobResult::Event(ref r) => {
                                let Some(result_sender) = self.q.get(&r.job_id) else {
                                    continue;
                                };

                                let send_result = result_sender.send(result).await;
                                tracing::debug!("Sent result to channel ({send_result:?})");
                            },
                            BatchJobResult::Done(id) => {
                                let Some(sender) = self.q.remove(&id) else {
                                    continue;
                                };
                                let _ = sender.send(result).await;
                                // Wait for all the messages to be sent, when the receiver drops,
                                // this will complete
                                sender.closed().await;
                                tracing::debug!("Job '{id}' finished, removing from queue");
                                continue;
                            }
                        };

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
                            result: Err(e),
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
                    result: Err(e),
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
                    .create_text_embeddings(EmbedTextInput::new(
                        document_id,
                        collection.collection.id
                    ))
                    .await
            );

            let result = JobEvent {
                job_id,
                result: Ok(EmbeddingReportType::TextAddition(report)),
            };

            result_tx.send(BatchJobResult::Event(result)).await.unwrap();
        }

        for document_id in remove.into_iter() {
            let report = ok_or_continue!(
                services
                    .embedding
                    .delete_text_embeddings(collection.collection.id, document_id)
                    .await
            );

            let result = JobEvent {
                job_id,
                result: Ok(EmbeddingReportType::TextRemoval(report)),
            };

            result_tx.send(BatchJobResult::Event(result)).await.unwrap();
        }

        let _ = result_tx.send(BatchJobResult::Done(job_id)).await;
    }
}

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
    result_tx: mpsc::Sender<BatchJobResult>,
}

impl BatchJob {
    pub fn new(
        collection: Uuid,
        add: Vec<Uuid>,
        remove: Vec<Uuid>,
        result_tx: mpsc::Sender<BatchJobResult>,
    ) -> Self {
        Self {
            collection,
            add,
            remove,
            result_tx,
        }
    }
}

/// Used internally to track the status of an embedding job.
/// If a `Done` is received, the job is removed from the executor's queue.
#[derive(Debug)]
pub enum BatchJobResult {
    /// Represents a finished removal or addition job with respect to the document.
    Event(JobEvent),

    /// Represents a completely finished job.
    Done(Uuid),
}

/// Represents a single document embedding result in a job.
#[derive(Debug)]
pub struct JobEvent {
    /// ID of the job the embedding happened in.
    job_id: Uuid,

    /// Result of the embedding process.
    pub result: Result<EmbeddingReportType, ChonkitError>,
}

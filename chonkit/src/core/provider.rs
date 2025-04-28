use super::{
    document::store::DocumentStorage, embeddings::Embedder, image::ImageStore, vector::VectorDb,
};
use crate::{err, error::ChonkitError};
use std::{collections::HashMap, sync::Arc};

pub type VectorDbProvider = ProviderFactory<Arc<dyn VectorDb + Send + Sync>>;
pub type EmbeddingProvider = ProviderFactory<Arc<dyn Embedder + Send + Sync>>;
pub type DocumentStorageProvider = ProviderFactory<Arc<dyn DocumentStorage + Send + Sync>>;

/// Used to track provider IDs.
pub trait Identity {
    fn id(&self) -> &'static str;
}

/// Provider factories are used to decouple concrete implementations from the business logic.
///
/// The concrete instances are always obtained from aggregate roots, i.e. [Documents][crate::core::model::document::Document]
/// or [Collections][crate::core::model::collection::Collection].
#[derive(Clone)]
pub struct ProviderFactory<T> {
    providers: HashMap<&'static str, T>,
}

impl<T> Default for ProviderFactory<T> {
    fn default() -> Self {
        Self {
            providers: HashMap::new(),
        }
    }
}

impl<T> ProviderFactory<T> {
    /// Get a provider from this factory.
    pub fn get_provider(&self, input: &str) -> Result<T, ChonkitError>
    where
        T: Clone,
    {
        match self.providers.get(input) {
            Some(e) => Ok(e.clone()),
            None => err!(InvalidProvider, "{input}"),
        }
    }

    /// List all registered provider IDs.
    pub fn list_provider_ids(&self) -> Vec<&'static str> {
        self.providers.keys().cloned().collect()
    }

    /// Register a provider in this factory.
    pub fn register(&mut self, provider: T)
    where
        T: Identity,
    {
        self.providers.insert(provider.id(), provider);
    }
}

/// Holds the factories for all available 3rd party service providers.
/// Chonkit services use this to obtain concrete implementations of their dependencies.
#[derive(Clone)]
pub struct ProviderState {
    /// Vector database providers.
    pub vector: ProviderFactory<Arc<dyn VectorDb + Send + Sync>>,

    /// Embedding providers.
    pub embedding: ProviderFactory<Arc<dyn Embedder + Send + Sync>>,

    // Document storage providers.
    pub document: ProviderFactory<Arc<dyn DocumentStorage + Send + Sync>>,

    /// Image storage providers.
    pub image: ImageStore,
}

impl<T> Identity for Arc<T>
where
    T: Identity,
{
    fn id(&self) -> &'static str {
        <T as Identity>::id(self)
    }
}

macro_rules! impl_identity {
    ($($t:ident),+) => {
        $(
            impl Identity for Arc<dyn $t + Send + Sync> {
                fn id(&self) -> &'static str {
                    <dyn $t as Identity>::id(self.as_ref())
                }
            }
        )+
    };
}

impl_identity!(VectorDb, Embedder, DocumentStorage);

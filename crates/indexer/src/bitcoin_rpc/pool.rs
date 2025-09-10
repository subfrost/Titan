use std::{
    collections::VecDeque,
    ops::{Deref, DerefMut},
    sync::{Arc, RwLock},
};

use bitcoincore_rpc::Client;

use super::{RpcClientError, RpcClientProvider};

// Wrapper that automatically returns the client to the pool when dropped
pub struct PooledClient {
    client: Option<Client>,
    pool: Arc<RpcClientPool>,
}

impl Deref for PooledClient {
    type Target = Client;

    fn deref(&self) -> &Self::Target {
        self.client.as_ref().expect("Client was taken")
    }
}

impl DerefMut for PooledClient {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.client.as_mut().expect("Client was taken")
    }
}

impl Drop for PooledClient {
    fn drop(&mut self) {
        // Take the client out to avoid double-free
        if let Some(client) = self.client.take() {
            // Try to return it to the pool, ignoring errors
            let _ = self.pool.release(client);
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RpcClientPoolError {
    #[error("lock poisoned")]
    LockPoisoned,

    #[error("provider error: {0}")]
    Provider(#[from] RpcClientError),
}

#[derive(Clone)]
pub struct RpcClientPool {
    clients: Arc<RwLock<VecDeque<Client>>>,
    max_size: usize,
    provider: Arc<dyn RpcClientProvider>,
}

impl RpcClientPool {
    pub fn new(provider: Arc<dyn RpcClientProvider>, max_size: usize) -> Self {
        Self {
            clients: Arc::new(RwLock::new(VecDeque::new())),
            max_size,
            provider,
        }
    }

    pub fn get(&self) -> Result<PooledClient, RpcClientPoolError> {
        let mut clients = self
            .clients
            .write()
            .map_err(|_| RpcClientPoolError::LockPoisoned)?;

        let client = if let Some(client) = clients.pop_front() {
            client
        } else {
            // Create a new client if the pool is empty
            self.provider.get_new_rpc_client()?
        };

        Ok(PooledClient {
            client: Some(client),
            pool: Arc::new(self.clone()),
        })
    }

    fn release(&self, client: Client) -> Result<(), RpcClientPoolError> {
        let mut clients = self
            .clients
            .write()
            .map_err(|_| RpcClientPoolError::LockPoisoned)?;

        // Only keep up to max_size clients
        if clients.len() < self.max_size {
            clients.push_back(client);
        }

        Ok(())
    }
}

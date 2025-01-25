use {
    super::store::{Store, StoreError},
    crate::models::Subscription,
    std::sync::Arc,
    uuid::Uuid,
};

pub struct SubscriptionManager {
    store: Arc<dyn Store>,
}

impl SubscriptionManager {
    pub fn new(store: Arc<dyn Store>) -> Self {
        Self { store }
    }

    pub fn add_subscription(&self, subscription: &Subscription) -> Result<(), StoreError> {
        self.store.set_subscription(subscription)
    }

    pub fn delete_subscription(&self, id: &Uuid) -> Result<(), StoreError> {
        self.store.delete_subscription(id)
    }

    pub fn get_subscriptions(&self) -> Result<Vec<Subscription>, StoreError> {
        self.store.get_subscriptions()
    }

    pub fn get_subscription(&self, id: &Uuid) -> Result<Subscription, StoreError> {
        self.store.get_subscription(id)
    }
}

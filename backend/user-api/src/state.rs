//! Service state shared by the HTTP layer and the GraphQL schema.

use std::sync::Arc;

use async_graphql::{EmptySubscription, Schema};
use common::clock::Clock;
use common::settings::UserApiSettings;

use crate::graphql::{MutationRoot, QueryRoot};
use crate::store::Store;

pub type UserApiSchema = Schema<QueryRoot, MutationRoot, EmptySubscription>;

#[derive(Clone)]
pub struct AppState {
    pub store: Store,
    pub settings: Arc<UserApiSettings>,
    pub clock: Arc<dyn Clock>,
}

impl AppState {
    pub fn new(store: Store, settings: UserApiSettings, clock: Arc<dyn Clock>) -> Self {
        Self {
            store,
            settings: Arc::new(settings),
            clock,
        }
    }
}

#[derive(Clone)]
pub struct App {
    pub state: AppState,
    pub schema: Arc<UserApiSchema>,
}

impl App {
    pub fn new(state: AppState) -> Self {
        let schema = Arc::new(crate::graphql::build_schema(state.clone()));
        Self { state, schema }
    }
}

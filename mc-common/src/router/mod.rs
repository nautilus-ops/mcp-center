use crate::app::AppState;
use axum::Router;

pub type RouterHandler<T> = Box<dyn Fn(Router<T>) -> Router<T> + Send + Sync>;

pub struct RouterBuilder<S: Clone + Send + Sync + 'static> {
    router: Router<S>,
}

impl<S: Clone + Send + Sync + 'static> RouterBuilder<S> {
    pub fn new() -> RouterBuilder<S> {
        let router = Router::<S>::new();
        RouterBuilder { router }
    }
    pub fn with_register(mut self, handler: RouterHandler<S>) -> RouterBuilder<S> {
        self.router = handler(self.router.clone());
        self
    }

    pub fn with_layer(mut self, handler: RouterHandler<S>) -> RouterBuilder<S> {
        self.router = handler(self.router.clone());
        self
    }

    pub fn build(self, state: S) -> Router {
        let router = self.router.clone();
        router.with_state(state)
    }
}

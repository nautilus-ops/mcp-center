use axum::routing::post;
use mc_common::app::AppState;
use mc_common::router;

mod token;

pub fn register_router() -> router::RouterHandler<AppState> {
    Box::new(|router| router.route("/api/user/admin/login", post(token::admin_login)))
}

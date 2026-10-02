//of-load: stress API whose levels burn CPU and hold memory behind the of-api bearer token.

mod auth;
mod burn;
mod config;
mod levels;
mod routes;
mod state;

pub use routes::router;
pub use state::AppState;

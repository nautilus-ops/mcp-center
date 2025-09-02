use crate::reverse_proxy::connection::ConnectionService;
use crate::reverse_proxy::message::MessageService;
use axum::Router;
use axum::body::Body;
use axum::http::StatusCode;
use axum::response::Response;
use axum::routing::Route;
use bytes::Bytes;
use http_body_util::StreamBody;
use hyper::body::Frame;
use hyper_rustls::HttpsConnector;
use hyper_util::client::legacy::Client;
use hyper_util::client::legacy::connect::HttpConnector;
use mc_common::app::AppState;
use mc_common::app::cache::Cache;
use mc_common::router;
use std::sync::Arc;
use tokio::sync::mpsc::Sender;
use tokio_stream::wrappers::ReceiverStream;

pub mod connection;
pub mod message;

type ProxyResponse = Response<StreamBody<ReceiverStream<Result<Frame<Bytes>, std::io::Error>>>>;

pub fn build_error_stream_response(
    tx: Sender<Result<Frame<Bytes>, std::io::Error>>,
    stream: ReceiverStream<Result<Frame<Bytes>, std::io::Error>>,
    msg: String,
    status: StatusCode,
) -> ProxyResponse {
    tokio::task::spawn(async move {
        tx.send(Ok(Frame::data(Bytes::from(msg)))).await.unwrap();
    });

    let mut response_builder = Response::builder();
    response_builder = response_builder.status(status);
    response_builder.body(StreamBody::new(stream)).unwrap()
}

pub fn register_router<S: Clone + Send + Sync + 'static>(
    client: Arc<Client<HttpsConnector<HttpConnector>, Body>>,
    cache: Arc<Cache>,
) -> router::RouterHandler<S> {
    Box::new(move |router: Router<S>| {
        router
            .route_service(
                "/proxy/connect/{name}/{tag}",
                ConnectionService::new(client.clone(), cache.clone()),
            )
            .route_service(
                "/proxy/message/{name}/{tag}/{*subPath}",
                MessageService::new(client.clone(), cache.clone()),
            )
    })
}

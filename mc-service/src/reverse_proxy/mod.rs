use axum::response::Response;
use bytes::Bytes;
use http::StatusCode;
use http_body_util::StreamBody;
use hyper::body::Frame;
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

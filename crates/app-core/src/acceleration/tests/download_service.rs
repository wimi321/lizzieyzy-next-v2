use super::{support::serve, within};
use std::{
    collections::BTreeMap,
    future::{Future, poll_fn},
    task::Poll,
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::oneshot,
};

#[tokio::test]
async fn incomplete_request_disconnect_preserves_server_and_next_download() {
    let listener = within(TcpListener::bind("127.0.0.1:0")).await.unwrap();
    let address = listener.local_addr().unwrap();
    let (stop, stopped) = oneshot::channel();
    let (release, gate) = oneshot::channel();
    let server = tokio::spawn(serve(
        listener,
        BTreeMap::from([("/archive".into(), b"test archive".to_vec())]),
        stopped,
        Some(gate),
    ));

    let mut cancelled = within(TcpStream::connect(address)).await.unwrap();
    within(cancelled.write_all(b"GET /archive HTTP/1.1\r\nHost: "))
        .await
        .unwrap();
    within(cancelled.shutdown()).await.unwrap();
    let mut response = Vec::new();
    // EOF acknowledges that the server discarded this connection before the
    // next request. The response gate must remain untouched by the partial head.
    within(cancelled.read_to_end(&mut response)).await.unwrap();
    assert!(response.is_empty());

    let mut next = within(TcpStream::connect(address)).await.unwrap();
    within(next.write_all(b"GET /archive HTTP/1.1\r\nHost: localhost\r\n\r\n"))
        .await
        .unwrap();
    release.send(()).unwrap();
    within(next.read_to_end(&mut response)).await.unwrap();
    assert_eq!(
        response,
        b"HTTP/1.1 200 OK\r\nContent-Length: 12\r\nConnection: close\r\n\r\ntest archive"
    );

    stop.send(()).unwrap();
    assert_eq!(within(server).await.unwrap(), 1);
}

#[tokio::test]
async fn idle_download_service_survives_between_bounded_operations() {
    let listener = within(TcpListener::bind("127.0.0.1:0")).await.unwrap();
    let address = listener.local_addr().unwrap();
    let (stop, stopped) = oneshot::channel();
    let mut service = Box::pin(serve(
        listener,
        BTreeMap::from([("/archive".into(), b"test archive".to_vec())]),
        stopped,
        None,
    ));
    // Poll the idle service ourselves so advancing time cannot race a spawned
    // server or a real connection. A fixture may span many bounded operations.
    poll_fn(|cx| {
        assert!(service.as_mut().poll(cx).is_pending());
        Poll::Ready(())
    })
    .await;
    tokio::time::pause();
    tokio::time::advance(Duration::from_secs(6)).await;
    tokio::time::resume();
    poll_fn(|cx| {
        assert!(service.as_mut().poll(cx).is_pending());
        Poll::Ready(())
    })
    .await;

    let server = tokio::spawn(service);
    let mut client = within(TcpStream::connect(address)).await.unwrap();
    within(client.write_all(b"GET /archive HTTP/1.1\r\nHost: localhost\r\n\r\n"))
        .await
        .unwrap();
    let mut response = Vec::new();
    within(client.read_to_end(&mut response)).await.unwrap();
    assert_eq!(
        response,
        b"HTTP/1.1 200 OK\r\nContent-Length: 12\r\nConnection: close\r\n\r\ntest archive"
    );
    stop.send(()).unwrap();
    assert_eq!(within(server).await.unwrap(), 1);
}

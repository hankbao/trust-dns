extern crate futures;
extern crate tokio;
extern crate trust_dns_proto;
extern crate trust_dns_server;

use std::io;
use std::time::Duration;
use futures::Poll;
#[allow(deprecated)]
use futures::stream::{iter, Stream};
use tokio::runtime::current_thread::Runtime;

use trust_dns_server::server::TimeoutStream;

#[test]
fn test_no_timeout() {
    #[allow(deprecated)]
    let sequence =
        iter(vec![Ok(1), Err("error"), Ok(2)]).map_err(|e| io::Error::new(io::ErrorKind::Other, e));
    let mut core = Runtime::new().expect("could not get core");

    let timeout_stream = TimeoutStream::new(sequence, Duration::from_secs(360));

    let (val, timeout_stream) = core.block_on(timeout_stream.into_future())
        .ok()
        .expect("first run failed");
    assert_eq!(val, Some(1));

    let error = core.block_on(timeout_stream.into_future());
    assert!(error.is_err());

    let (_, timeout_stream) = error.err().unwrap();

    let (val, timeout_stream) = core.block_on(timeout_stream.into_future())
        .ok()
        .expect("third run failed");
    assert_eq!(val, Some(2));

    let (val, _) = core.block_on(timeout_stream.into_future())
        .ok()
        .expect("fourth run failed");
    assert!(val.is_none())
}

struct NeverStream {}

impl Stream for NeverStream {
    type Item = Result<(), io::Error>;

    // somehow insert a timeout here...
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Result<Self::Item, Self::Error>>> {
        Poll::Pending
    }
}

#[test]
fn test_timeout() {
    let mut core = Runtime::new().expect("could not get core");
    let timeout_stream = TimeoutStream::new(NeverStream {}, Duration::from_millis(1));

    assert!(core.block_on(timeout_stream.into_future()).is_err());
}

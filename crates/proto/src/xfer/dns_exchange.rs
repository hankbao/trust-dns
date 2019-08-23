// Copyright 2015-2018 Benjamin Fry <benjaminfry@me.com>
//
// Licensed under the Apache License, Version 2.0, <LICENSE-APACHE or
// http://apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT or
// http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.

//! This module contains all the types for demuxing DNS oriented streams.

use std::pin::Pin;
use std::task::Context;

use futures::stream::{Peekable, Stream};
use futures::{Future, Poll};
use tokio_sync::mpsc::{unbounded_channel, UnboundedReceiver};

use crate::error::*;
use crate::xfer::{DnsRequest, DnsRequestSender, DnsRequestStreamHandle, DnsResponse, OneshotDnsRequest};

/// This is a generic Exchange implemented over multiplexed DNS connection providers.
///
/// The underlying `DnsRequestSender` is expected to multiplex any I/O connections. DnsExchange assumes that the underlying stream is responsible for this.
#[must_use = "futures do nothing unless polled"]
pub struct DnsExchange<S, R>
where
    S: DnsRequestSender<DnsResponseFuture = R>,
    R: Future<Output = Result<DnsResponse, ProtoError>> + 'static + Send,
{
    io_stream: S,
    outbound_messages: Peekable<UnboundedReceiver<OneshotDnsRequest<R>>>,
}

impl<S, R> DnsExchange<S, R>
where
    S: DnsRequestSender<DnsResponseFuture = R>,
    R: Future<Output = Result<DnsResponse, ProtoError>> + 'static + Send,
{
    /// Initializes a TcpStream with an existing tokio_tcp::TcpStream.
    ///
    /// This is intended for use with a TcpListener and Incoming.
    ///
    /// # Arguments
    ///
    /// * `stream` - the established IO stream for communication
    pub fn from_stream(stream: S) -> (Self, DnsRequestStreamHandle<R>) {
        let (message_sender, outbound_messages) = unbounded_channel();
        let message_sender = DnsRequestStreamHandle::<R>::new(message_sender);

        let stream = Self::from_stream_with_receiver(stream, outbound_messages);

        (stream, message_sender)
    }

    /// Wraps a stream where a sender and receiver have already been established
    pub fn from_stream_with_receiver(
        stream: S,
        receiver: UnboundedReceiver<OneshotDnsRequest<R>>,
    ) -> Self {
        DnsExchange {
            io_stream: stream,
            outbound_messages: receiver.peekable(),
        }
    }

    /// Returns a future, which itself wraps a future which is awaiting connection.
    ///
    /// The connect_future should be lazy.
    pub fn connect<F>(connect_future: F) -> (DnsExchangeConnect<F, S, R>, DnsRequestStreamHandle<R>)
    where
        F: Future<Output = Result<S, ProtoError>> + 'static + Send,
    {
        let (message_sender, outbound_messages) = unbounded_channel();
        (
            DnsExchangeConnect::connect(connect_future, outbound_messages),
            DnsRequestStreamHandle::<R>::new(message_sender),
        )
    }
}

impl<S, R> Future for DnsExchange<S, R>
where
    S: DnsRequestSender<DnsResponseFuture = R>,
    R: Future<Output = Result<DnsResponse, ProtoError>> + 'static + Send,
{
    type Output = Result<(), ProtoError>;

    #[allow(clippy::unused_unit)]
    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        // this will not accept incoming data while there is data to send
        //  makes this self throttling.
        loop {
            // poll the underlying stream, to drive it...
            match self.io_stream.poll() {
                // The stream is ready
                Poll::Ready(Some(Ok(()))) => (),
                Poll::Pending => {
                    if self.io_stream.is_shutdown() {
                        // the io_stream is in a shutdown state, we are only waiting for final results...
                        return Poll::Pending;
                    }

                    // NotReady and not shutdown, see if there are more messages to send
                    ()
                } // underlying stream is complete.
                Poll::Ready(None) => {
                    debug!("io_stream is done, shutting down");
                    // TODO: return shutdown error to anything in the stream?

                    return Poll::Ready(Some(Ok()));
                }
                Err(err) => return Poll::Ready(Some(Err(err))),
            }

            // then see if there is more to send
            match self
                .outbound_messages
                .poll()
                .map_err(|()| ProtoError::from("unknown from outbound_message receiver"))?
            {
                // already handled above, here to make sure the poll() pops the next message
                Poll::Ready(Some(dns_request)) => {
                    // if there is no peer, this connection should die...
                    let (dns_request, serial_response): (DnsRequest, _) = dns_request.unwrap();

                    debug!("sending message via: {}", self.io_stream);

                    match serial_response.send_response(self.io_stream.send_message(dns_request)) {
                        Ok(()) => (),
                        Err(_) => {
                            warn!("failed to associate send_message response to the sender");
                            return Err(
                                "failed to associate send_message response to the sender".into()
                            );
                        }
                    }
                }
                // On not ready, this is our time to return...
                Poll::Pending => return Poll::Pending,
                Poll::Ready(None) => {
                    debug!("all handles closed, shutting down: {}", self.io_stream);
                    // if there is nothing that can use this connection to send messages, then this is done...
                    self.io_stream.shutdown();

                    // now we'll await the stream to shutdown... see io_stream poll above
                }
            }

            // else we loop to poll on the outbound_messages
        }
    }
}

/// A wrapper for a future DnsExchange connection
pub struct DnsExchangeConnect<F, S, R>(DnsExchangeConnectInner<F, S, R>)
where
    F: Future<Output = Result<S, ProtoError>> + 'static + Send,
    S: DnsRequestSender<DnsResponseFuture = R>,
    R: Future<Output = Result<DnsResponse, ProtoError>> + 'static + Send;

impl<F, S, R> DnsExchangeConnect<F, S, R>
where
    F: Future<Output = Result<S, ProtoError>> + 'static + Send,
    S: DnsRequestSender<DnsResponseFuture = R>,
    R: Future<Output = Result<DnsResponse, ProtoError>> + 'static + Send,
{
    fn connect(
        connect_future: F,
        outbound_messages: UnboundedReceiver<OneshotDnsRequest<R>>,
    ) -> Self {
        DnsExchangeConnect(DnsExchangeConnectInner::Connecting {
            connect_future,
            outbound_messages: Some(outbound_messages),
        })
    }
}

impl<F, S, R> Future for DnsExchangeConnect<F, S, R>
where
    F: Future<Output = Result<S, ProtoError>> + 'static + Send,
    S: DnsRequestSender<DnsResponseFuture = R>,
    R: Future<Output = Result<DnsResponse, ProtoError>> + 'static + Send,
{
    type Output = Result<DnsExchange<S, R>, ProtoError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        self.0.poll()
    }
}

enum DnsExchangeConnectInner<F, S, R>
where
    F: Future<Output = Result<S, ProtoError>> + 'static + Send,
    S: DnsRequestSender<DnsResponseFuture = R>,
    R: Future<Output = Result<DnsResponse, ProtoError>> + 'static + Send,
{
    Connecting {
        connect_future: F,
        outbound_messages: Option<UnboundedReceiver<OneshotDnsRequest<R>>>,
    },
    FailAll {
        error: ProtoError,
        outbound_messages: UnboundedReceiver<OneshotDnsRequest<R>>,
    },
}

impl<F, S, R> Future for DnsExchangeConnectInner<F, S, R>
where
    F: Future<Output = Result<S, ProtoError>> + 'static + Send,
    S: DnsRequestSender<DnsResponseFuture = R>,
    R: Future<Output = Result<DnsResponse, ProtoError>> + 'static + Send,
{
    type Output = Result<DnsExchange<S, R>, ProtoError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        loop {
            let next;
            match self {
                DnsExchangeConnectInner::Connecting {
                    ref mut connect_future,
                    ref mut outbound_messages,
                } => {
                    match connect_future.poll() {
                        Poll::Ready(Ok(stream)) => {
                            debug!("connection established: {}", stream);
                            return Poll::Ready(Ok(DnsExchange::from_stream_with_receiver(
                                stream,
                                outbound_messages
                                    .take()
                                    .expect("cannot poll after complete"),
                            )));
                        }
                        Poll::Pending => return Poll::Pending,
                        Poll::Ready(Err(error)) => {
                            debug!("stream errored while connecting: {:?}", error);
                            next = DnsExchangeConnectInner::FailAll {
                                error,
                                outbound_messages: outbound_messages
                                    .take()
                                    .expect("cannot poll after complete"),
                            }
                        }
                    };
                }
                DnsExchangeConnectInner::FailAll {
                    error,
                    ref mut outbound_messages,
                } => {
                    while let Some(outbound_message) = match outbound_messages.poll() {
                        Poll::Ready(Ok(opt)) => opt,
                        Poll::Pending => return Poll::Pending,
                        Poll::Ready(Err(_)) => None,
                    } {
                        let response = S::error_response(error.clone());
                        // ignoring errors... best effort send...
                        outbound_message.unwrap().1.send_response(response).ok();
                    }

                    return Poll::Ready(Err(error.clone()));
                }
            }

            *self = next;
        }
    }
}

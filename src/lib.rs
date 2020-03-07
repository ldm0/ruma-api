//! Crate `ruma_api` contains core types used to define the requests and responses for each endpoint
//! in the various [Matrix](https://matrix.org) API specifications.
//! These types can be shared by client and server code for all Matrix APIs.
//!
//! When implementing a new Matrix API, each endpoint has a request type which implements
//! `Endpoint`, and a response type connected via an associated type.
//!
//! An implementation of `Endpoint` contains all the information about the HTTP method, the path and
//! input parameters for requests, and the structure of a successful response.
//! Such types can then be used by client code to make requests, and by server code to fulfill
//! those requests.

#![warn(rust_2018_idioms)]
#![deny(missing_copy_implementations, missing_debug_implementations, missing_docs)]

use std::convert::{TryFrom, TryInto};

use http::Method;

#[cfg(feature = "with-ruma-api-macros")]
pub use ruma_api_macros::ruma_api;

#[cfg(feature = "with-ruma-api-macros")]
pub use ruma_api_macros::Outgoing;

pub mod error;
/// This module is used to support the generated code from ruma-api-macros.
/// It is not considered part of ruma-api's public API.
#[cfg(feature = "with-ruma-api-macros")]
#[doc(hidden)]
pub mod exports {
    pub use http;
    pub use percent_encoding;
    pub use serde;
    pub use serde_json;
    pub use serde_urlencoded;
    pub use url;
}

use error::{FromHttpRequestError, FromHttpResponseError, IntoHttpError};

/// A type that can be sent to another party that understands the matrix protocol. If any of the
/// fields of `Self` don't implement serde's `Deserialize`, you can derive this trait to generate a
/// corresponding 'Incoming' type that supports deserialization. This is useful for things like
/// ruma_events' `EventResult` type. For more details, see the [derive macro's documentation][doc].
///
/// [doc]: derive.Outgoing.html
// TODO: Better explain how this trait relates to serde's traits
pub trait Outgoing {
    /// The 'Incoming' variant of `Self`.
    type Incoming;
}

/// A Matrix API endpoint.
///
/// The type implementing this trait contains any data needed to make a request to the endpoint.
pub trait Endpoint: Outgoing + TryInto<http::Request<Vec<u8>>, Error = IntoHttpError>
where
    <Self as Outgoing>::Incoming: TryFrom<http::Request<Vec<u8>>, Error = FromHttpRequestError>,
    <Self::Response as Outgoing>::Incoming:
        TryFrom<http::Response<Vec<u8>>, Error = FromHttpResponseError>,
{
    /// Data returned in a successful response from the endpoint.
    type Response: Outgoing + TryInto<http::Response<Vec<u8>>, Error = IntoHttpError>;

    /// Metadata about the endpoint.
    const METADATA: Metadata;
}

/// Metadata about an API endpoint.
#[derive(Clone, Debug)]
pub struct Metadata {
    /// A human-readable description of the endpoint.
    pub description: &'static str,

    /// The HTTP method used by this endpoint.
    pub method: Method,

    /// A unique identifier for this endpoint.
    pub name: &'static str,

    /// The path of this endpoint's URL, with variable names where path parameters should be filled
    /// in during a request.
    pub path: &'static str,

    /// Whether or not this endpoint is rate limited by the server.
    pub rate_limited: bool,

    /// Whether or not the server requires an authenticated user for this endpoint.
    pub requires_authentication: bool,
}

#[cfg(test)]
mod tests {
    /// PUT /_matrix/client/r0/directory/room/:room_alias
    pub mod create {
        use std::{convert::TryFrom, ops::Deref};

        use http::{header::CONTENT_TYPE, method::Method};
        use ruma_identifiers::{RoomAliasId, RoomId};
        use serde::{Deserialize, Serialize};

        use crate::{
            error::{
                FromHttpRequestError, FromHttpResponseError, IntoHttpError,
                RequestDeserializationError, ServerError,
            },
            Endpoint, Metadata, Outgoing,
        };

        /// A request to create a new room alias.
        #[derive(Debug)]
        pub struct Request {
            pub room_id: RoomId,         // body
            pub room_alias: RoomAliasId, // path
        }

        impl Outgoing for Request {
            type Incoming = Self;
        }

        impl Endpoint for Request {
            type Response = Response;

            const METADATA: Metadata = Metadata {
                description: "Add an alias to a room.",
                method: Method::PUT,
                name: "create_alias",
                path: "/_matrix/client/r0/directory/room/:room_alias",
                rate_limited: false,
                requires_authentication: true,
            };
        }

        impl TryFrom<Request> for http::Request<Vec<u8>> {
            type Error = IntoHttpError;

            fn try_from(request: Request) -> Result<http::Request<Vec<u8>>, Self::Error> {
                let metadata = Request::METADATA;

                let path = metadata
                    .path
                    .to_string()
                    .replace(":room_alias", &request.room_alias.to_string());

                let request_body = RequestBody { room_id: request.room_id };

                let http_request = http::Request::builder()
                    .method(metadata.method)
                    .uri(path)
                    .body(serde_json::to_vec(&request_body)?)
                    .expect("http request building to succeed");

                Ok(http_request)
            }
        }

        impl TryFrom<http::Request<Vec<u8>>> for Request {
            type Error = FromHttpRequestError;

            fn try_from(request: http::Request<Vec<u8>>) -> Result<Self, Self::Error> {
                let request_body: RequestBody =
                    match serde_json::from_slice(request.body().as_slice()) {
                        Ok(body) => body,
                        Err(err) => {
                            return Err(RequestDeserializationError::new(err, request).into());
                        }
                    };
                let path_segments: Vec<&str> = request.uri().path()[1..].split('/').collect();
                Ok(Request {
                    room_id: request_body.room_id,
                    room_alias: {
                        let segment = path_segments.get(5).unwrap().as_bytes();
                        let decoded = match percent_encoding::percent_decode(segment).decode_utf8()
                        {
                            Ok(x) => x,
                            Err(err) => {
                                return Err(RequestDeserializationError::new(err, request).into())
                            }
                        };
                        match serde_json::from_str(decoded.deref()) {
                            Ok(id) => id,
                            Err(err) => {
                                return Err(RequestDeserializationError::new(err, request).into())
                            }
                        }
                    },
                })
            }
        }

        #[derive(Debug, Serialize, Deserialize)]
        struct RequestBody {
            room_id: RoomId,
        }

        /// The response to a request to create a new room alias.
        #[derive(Clone, Copy, Debug)]
        pub struct Response;

        impl Outgoing for Response {
            type Incoming = Self;
        }

        impl TryFrom<http::Response<Vec<u8>>> for Response {
            type Error = FromHttpResponseError;

            fn try_from(http_response: http::Response<Vec<u8>>) -> Result<Response, Self::Error> {
                if http_response.status().as_u16() < 400 {
                    Ok(Response)
                } else {
                    Err(FromHttpResponseError::Http(ServerError::new(http_response)))
                }
            }
        }

        impl TryFrom<Response> for http::Response<Vec<u8>> {
            type Error = IntoHttpError;

            fn try_from(_: Response) -> Result<http::Response<Vec<u8>>, Self::Error> {
                let response = http::Response::builder()
                    .header(CONTENT_TYPE, "application/json")
                    .body(b"{}".to_vec())
                    .unwrap();

                Ok(response)
            }
        }
    }
}

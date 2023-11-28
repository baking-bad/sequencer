// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anemo::rpc::Status;
use anemo::{Request, Response};
use anemo_tower::auth::AuthorizeRequest;
use bytes::Bytes;

/// The epoch header attached to all network requests.
pub const EPOCH_HEADER_KEY: &str = "epoch";

#[derive(Clone, Debug)]
pub struct AllowedEpoch {
    allowed_epoch: String,
}

impl AllowedEpoch {
    pub fn new(epoch: String) -> Self {
        Self {
            allowed_epoch: epoch,
        }
    }
}

impl AuthorizeRequest for AllowedEpoch {
    fn authorize(&self, request: &mut Request<Bytes>) -> Result<(), Response<Bytes>> {
        use anemo::types::response::{IntoResponse, StatusCode};

        let epoch = request.headers().get(EPOCH_HEADER_KEY).ok_or_else(|| {
            Status::new_with_message(StatusCode::BadRequest, "missing epoch header").into_response()
        })?;

        if self.allowed_epoch == *epoch {
            Ok(())
        } else {
            Err(Status::new_with_message(
                StatusCode::BadRequest,
                format!(
                    "request from epoch {:?} does not match current epoch {:?}",
                    epoch, self.allowed_epoch
                ),
            )
            .into_response())
        }
    }
}

// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anemo::rpc::codec::{Codec, Decoder, Encoder};
use bytes::Buf;
use std::{io::Read, marker::PhantomData};

// Anemo variant of BCS codec using Snappy for compression.
#[derive(Debug)]
pub struct BcsSnappyEncoder<T>(PhantomData<T>);

impl<T: serde::Serialize> Encoder for BcsSnappyEncoder<T> {
    type Item = T;
    type Error = bcs::Error;

    fn encode(&mut self, item: Self::Item) -> Result<bytes::Bytes, Self::Error> {
        let mut buf = Vec::<u8>::new();
        let mut snappy_encoder = snap::write::FrameEncoder::new(&mut buf);
        bcs::serialize_into(&mut snappy_encoder, &item)?;
        drop(snappy_encoder);
        Ok(buf.into())
    }
}

#[derive(Debug)]
pub struct BcsSnappyDecoder<U>(PhantomData<U>);

impl<U: serde::de::DeserializeOwned> Decoder for BcsSnappyDecoder<U> {
    type Item = U;
    type Error = bcs::Error;

    fn decode(&mut self, buf: bytes::Bytes) -> Result<Self::Item, Self::Error> {
        let compressed_size = buf.len();
        let mut snappy_decoder = snap::read::FrameDecoder::new(buf.reader()).take(1 << 30);
        let mut bytes = Vec::with_capacity(compressed_size);
        snappy_decoder.read_to_end(&mut bytes)?;
        bcs::from_bytes(bytes.as_slice())
    }
}

/// A [`Codec`] that implements `bcs` encoding/decoding via the serde library.
#[derive(Debug, Clone)]
pub struct BcsSnappyCodec<T, U>(PhantomData<(T, U)>);

impl<T, U> Default for BcsSnappyCodec<T, U> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl<T, U> Codec for BcsSnappyCodec<T, U>
where
    T: serde::Serialize + Send + 'static,
    U: serde::de::DeserializeOwned + Send + 'static,
{
    type Encode = T;
    type Decode = U;
    type Encoder = BcsSnappyEncoder<T>;
    type Decoder = BcsSnappyDecoder<U>;

    fn encoder(&mut self) -> Self::Encoder {
        BcsSnappyEncoder(PhantomData)
    }

    fn decoder(&mut self) -> Self::Decoder {
        BcsSnappyDecoder(PhantomData)
    }

    fn format_name(&self) -> &'static str {
        "bcs"
    }
}
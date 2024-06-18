use rlp::DecoderError;

#[derive(Debug)]
pub enum Error {
    RlpDecoderError(DecoderError),
    InternalError(&'static str),
}

impl From<DecoderError> for Error {
    fn from(err: DecoderError) -> Self {
        Error::RlpDecoderError(err)
    }
}

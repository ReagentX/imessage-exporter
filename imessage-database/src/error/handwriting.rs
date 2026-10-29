/*!
 Errors that can happen when parsing `handwriting` data.
*/

use std::fmt::{Display, Formatter, Result};

/// Errors that can happen when parsing `handwriting` data
#[derive(Debug)]
pub enum HandwritingError {
    /// Wraps an error returned by the protobuf parser.
    ProtobufError(protobuf::Error),
    /// Indicates that the frame size was invalid.
    InvalidFrameSize(usize),
    /// Indicates that the frame's width or height is too large to render.
    InvalidDimensions(u16, u16),
    /// Wraps an error returned by the LZMA decompression.
    XZError(lzma_rs::error::Error),
    /// Indicates that the compression method is unknown.
    CompressionUnknown,
    /// Indicates that the strokes length is invalid.
    InvalidStrokesLength(usize, usize),
    /// Indicates that a stroke contained zero points.
    EmptyStroke,
    /// Indicates a numeric conversion error.
    ConversionError,
    /// Indicates that the decompressed data was not set.
    DecompressedNotSet,
    /// Indicates that the decompressed length is invalid.
    InvalidDecompressedLength(usize, usize),
    /// Wraps an error that occurred during resizing of handwriting coordinates.
    ResizeError(std::num::TryFromIntError),
}

impl std::error::Error for HandwritingError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            HandwritingError::ProtobufError(e) => Some(e),
            HandwritingError::XZError(e) => Some(e),
            HandwritingError::ResizeError(e) => Some(e),
            _ => None,
        }
    }
}

impl Display for HandwritingError {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> Result {
        match self {
            HandwritingError::ProtobufError(why) => {
                write!(fmt, "failed to parse handwriting protobuf: {why}")
            }
            HandwritingError::InvalidFrameSize(size) => write!(fmt, "expected size 8, got {size}"),
            HandwritingError::InvalidDimensions(width, height) => {
                write!(fmt, "frame dimensions {width}x{height} are out of range")
            }
            HandwritingError::XZError(why) => write!(fmt, "failed to decompress xz: {why}"),
            HandwritingError::CompressionUnknown => write!(fmt, "compress method unknown"),
            HandwritingError::InvalidStrokesLength(index, length) => {
                write!(fmt, "can't access index {index} on array length {length}")
            }
            HandwritingError::EmptyStroke => write!(fmt, "stroke contains zero points"),
            HandwritingError::ConversionError => write!(fmt, "failed to convert num"),
            HandwritingError::DecompressedNotSet => {
                write!(fmt, "decompressed length not set on compressed message")
            }
            HandwritingError::InvalidDecompressedLength(expected, got) => {
                write!(fmt, "expected decompressed length of {expected}, got {got}")
            }
            HandwritingError::ResizeError(why) => {
                write!(fmt, "failed to resize handwriting coordinates: {why}")
            }
        }
    }
}

/*!
 Media of the FaceTime conversation behind a FaceTime or SharePlay message.
*/

/// Media stored in the `conversationAVMode` key of an `item_type` 6 row's
/// `message_summary_info`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConversationAVMode {
    /// Value `0`: a SharePlay session without audio or video, as checked in
    /// `+[IMFaceTimeUtilities conversationIsAVLessSharePlay:]`.
    AVLess,
    /// Value `1`: an audio-only call, between none (`0`) and video (`2`),
    /// accepted in `-[TUConversationProvider supportsAVMode:]`.
    Audio,
    /// Value `2`: a video call, as checked in
    /// `+[IMFaceTimeUtilities conversationIsVideoCall:]`.
    Video,
    /// Unrecognized raw code.
    Unknown(u64),
}

impl ConversationAVMode {
    /// Convert a raw `conversationAVMode` code, preserving a missing value as
    /// `None`.
    #[must_use]
    pub fn from_code(code: Option<u64>) -> Option<Self> {
        Some(match code? {
            0 => Self::AVLess,
            1 => Self::Audio,
            2 => Self::Video,
            other => Self::Unknown(other),
        })
    }
}

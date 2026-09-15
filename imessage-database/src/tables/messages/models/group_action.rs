/*!
 Group actions encoded by a message row.
*/

use crate::tables::messages::message::Message;

/// Group action encoded by a message row.
#[derive(Debug, PartialEq, Eq)]
pub enum GroupAction<'a> {
    /// Participant was added to the group.
    ParticipantAdded(i32),
    /// Participant was removed from the group.
    ParticipantRemoved(i32),
    /// Group name changed.
    NameChange(&'a str),
    /// Participant left the group.
    ParticipantLeft,
    /// Group icon/avatar changed.
    GroupIconChanged,
    /// Group icon/avatar was removed.
    GroupIconRemoved,
    /// Chat background changed.
    ChatBackgroundChanged,
    /// Chat background was removed.
    ChatBackgroundRemoved,
    /// Participant changed their phone number.
    PhoneNumberChanged(i32),
}

impl<'a> GroupAction<'a> {
    /// Parse group action fields from a message row.
    #[must_use]
    pub(crate) fn from_message(message: &'a Message) -> Option<Self> {
        match (
            message.item_type,
            message.group_action_type,
            message.other_handle,
            &message.group_title,
        ) {
            // If the handle_id of the message matches the other_handle, the sender changed their own phone number
            (1, 0, Some(who), _) if message.handle_id == Some(who) => {
                Some(Self::PhoneNumberChanged(who))
            }
            (1, 0, Some(who), _) => Some(Self::ParticipantAdded(who)),
            (1, 1, Some(who), _) => Some(Self::ParticipantRemoved(who)),
            (2, _, _, Some(name)) => Some(Self::NameChange(name)),
            (3, 0, _, _) => Some(Self::ParticipantLeft),
            (3, 1, _, _) => Some(Self::GroupIconChanged),
            (3, 2, _, _) => Some(Self::GroupIconRemoved),
            (3, 4, _, _) => Some(Self::ChatBackgroundChanged),
            (3, 6, _, _) => Some(Self::ChatBackgroundRemoved),
            _ => None,
        }
    }
}

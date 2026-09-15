/*!
 Direction of a legacy shared-location event.
*/

/// Direction of a legacy shared-location event (`item_type == 4` with
/// `group_action_type == 0`). The two cases are mutually exclusive: the
/// underlying `share_status` bool distinguishes them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SharedLocation {
    /// The sender began sharing their location.
    Started,
    /// The sender stopped sharing their location.
    Stopped,
}

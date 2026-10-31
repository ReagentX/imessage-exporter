/*!
 Direction of a legacy shared-location event.
*/

/// Direction of a legacy shared-location event (`item_type == 4`), from
/// `share_status` `0` or `1`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SharedLocation {
    /// The sender began sharing their location.
    Started,
    /// The sender stopped sharing their location.
    Stopped,
}

/*!
 Message body models reconstructed from [`message.attributed_body`](crate::tables::messages::message::Message::attributed_body).

 Each model lives in its own submodule and is re-exported here. The submodules
 are private so `models::Service` stays the only path to each type: the file
 layout is ours to change, not API downstream code can depend on.
*/

pub use crate::tables::messages::models::{
    attachment_meta::AttachmentMeta, attributed_range::AttributedRange,
    bubble_component::BubbleComponent, filter_action::FilterAction, group_action::GroupAction,
    service::Service, shared_location::SharedLocation,
};

mod attachment_meta;
mod attributed_range;
mod bubble_component;
mod filter_action;
mod group_action;
mod service;
mod shared_location;

/*!
 Message body models reconstructed from `attributedBody` bytes during row decoding.
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

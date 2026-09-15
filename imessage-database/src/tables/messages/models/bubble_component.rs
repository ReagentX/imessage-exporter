/*!
 Per-part components of a parsed message body.
*/

use crate::tables::messages::models::attributed_range::AttributedRange;

/// Component emitted for one logical message part.
///
/// # Component Types
///
/// A single iMessage contains data that may be represented across multiple bubbles.
/// Each bubble corresponds to one `__kIMMessagePartAttributeName` index in the
/// underlying [`NSAttributedString`](crate::util::typedstream); the
/// [`Run`](Self::Run) groups every attributed range that shares that part index.
#[derive(Debug, PartialEq, Clone)]
pub enum BubbleComponent {
    /// One bubble's worth of attributed body content. Each contained
    /// [`AttributedRange`] models a single `NSAttributedString` attribute run
    /// (a byte range plus its attribute dictionary); adjacent ranges that
    /// share a `__kIMMessagePartAttributeName` index share the bubble.
    ///
    /// A run may interleave text ranges ([`AttributedRange::attachment`] is
    /// `None`) with inline-attachment ranges (e.g. stickers rendered inline
    /// like emoji), preserving their original order.
    Run(Vec<AttributedRange>),
    /// An [app integration](crate::message_types::app)
    App,
    /// A component that was retracted, found by parsing the [`EditedMessage`](crate::message_types::edited::EditedMessage)
    Retracted,
}

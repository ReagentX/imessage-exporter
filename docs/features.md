# Features

The library and binary crates aim to provide the most comprehensive and accurate representation of iMessage data available.

## Targeted Versions

This tool targets the current latest public release for Messages.app. It may work with older databases, but all features may not be available.

## Supported data sources

- Local macOS messages
- Encrypted or unencrypted local iOS backups
  - Unencrypted backups are resolved normally
  - Uses [crabapple](https://github.com/ReagentX/crabapple) to decrypt data from encrypted iOS backups

## Supported Message Features

- Plain Text
  - Correctly extracts time-zone corrected timestamps
  - Detects when a message was read and calculates the time until read for both parties
    - Humanizes display of time-until-read duration
  - Parses `typedstream` message body data
  - Detects the service a message was sent from
    - In HTML exports, balloons are colored correctly for the service they were sent with
    - Supports iMessage, SMS, MMS, and RCS
- Formatted Text
  - Parses formatted text ranges from `typedstream` message body data
  - Supports all iMessage text format ranges:
    - [Mentions](https://support.apple.com/guide/messages/mention-a-person-icht306ee34b/mac)
    - Hyperlinks
    - OTP/2FA
    - Unit Conversions
    - [Animations and Styles](https://support.apple.com/guide/iphone/style-and-animate-messages-iphe5c5af4d4/ios)
- Edited and Unsent messages
  - Detects if messages components were edited or unsent
    - [Edited messages](https://support.apple.com/guide/iphone/unsend-and-edit-messages-iphe67195653/ios)
      - Parses `typedstream` edited body data
      - Displays content and timestamps for each edit
      - Humanizes display of edit timestamp gaps
      - Edited messages received before Ventura display as normal messages without history
    - Unsent messages
      - No content, but are noted in context
- Multi-part messages
  - iMessages can have multiple parts, denoted by ranges in `typedstream` message body data
  - Parts are displayed as
    - New lines in TXT exports
    - Separate balloons in HTML exports
  - Handles Edited and Unsent parts
- Threads and Message Replies
  - [Threads](https://support.apple.com/en-us/104974) are displayed both threaded under the parent as well as in-place
    - This is to preserve context, which can be lost if replying to older messages
    - Messages from a thread and were rendered in-place are annotated as such
    - In HTML exports, threaded messages are hyperlinked to allow for easy reading in context
  - For multi-part messages, replies are threaded under the correct message part
- Attachments
  - Any type of attachment that can be displayed on the web is embedded in the HTML exports
  - Attachments can be copied to the export directory or referenced in-place
  - Less-compatible attachments can be converted for even more portable exports:
    - Image `HEIC` files convert to `JPEG`
    - Sticker `HEIC` files convert to `PNG`
    - Animated Sticker `HEICS` (HEIC sequence) files convert to `GIF`
    - Video `MOV` files convert to `mp4`
    - Audio `CAF` files convert to `mp4`
  - Attachments are displayed as
    - File paths in TXT exports
    - Embeds in HTML exports (including `<img>`, `<video>`, and `<audio>`)
      - [Audio messages](https://support.apple.com/guide/messages/send-an-audio-message-icht204ef108/mac) include embedded transcripts
  - Attachment date metadata is set to the date and time of message receipt
- Expressives
  - Detects both bubble and screen [effects](https://support.apple.com/en-us/104970)
  - Messages sent with expressives are annotated
- Tapbacks
  - Detects [tapbacks](https://support.apple.com/guide/iphone/react-with-tapbacks-iph018d3c336/ios) to messages
  - Messages sent or received with tapbacks are annotated
  - For multi-part messages, tapbacks are placed under the correct message part
- Stickers
  - Detects [stickers](https://support.apple.com/guide/iphone/send-stickers-iph37b0bfe7b/ios) sent or placed on messages
  - Messages sent with stickers are
    - Displayed in HTML exports
    - Annotated in TXT exports
  - For multi-part messages, stickers are placed under the correct message part
  - Sticker effects are annotated in all exports
  - Sticker tapbacks are also supported
- Apple Pay
  - Detects the transaction source, amount, and type
- URL previews
  - Parses the `NSKeyedArchiver` payload to extract preview data
    - Extracts cached metadata for each URL
    - Preview images display in HTML exports
    - URLs that have rotten may still retain some context if they have cached data
  - Handles cases where URL messages are overloaded with other message types
    - Apple Music (including preview streams and lyrics)
    - Apple Maps (including `Placemark` data)
    - App Store (including app metadata)
    - Rich Collaboration
- App Integrations
  - Parses the `NSKeyedArchiver` payload to extract balloon data
  - Supports system message types as well as third party [applications](https://support.apple.com/en-us/104969)
    - Apple Fitness messages
    - Photo Slideshow messages
    - SharePlay/Facetime messages
    - Check In messages
    - Find My messages
- Handwritten Messages
  - Parses the protobuf payload to extract [handwritten](https://support.apple.com/en-my/guide/iphone/iph3d4cb79c9/ios) message data
    - Displayed as embedded `svg` in HTML exports
    - TXT export behavior depends on attachment settings:
      - `disabled`: embedded inline as an `ascii` graphic
      - `clone, basic, full`: saved as an `svg` file
- Digital Touch
  - Parses the protobuf payload to extract [Digital Touch](https://support.apple.com/guide/ipod-touch/send-a-digital-touch-effect-iph3fadba219/ios) message data
    - Displayed as text that describes the type of message sent in HTML and TXT exports
- Duplicated group chats
  - Handles (participants) and chats (threads) can become duplicated
  - On startup:
    - Different handles that belong to the same person are combined
    - Chatrooms that contain identical contacts (i.e., duplicated handles) are combined

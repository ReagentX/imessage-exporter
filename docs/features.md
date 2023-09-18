# Features

This exporter is fully-featured and well-documented.

## Targeted Versions

This tool targets the current latest public release for macOS and iMessage. It may work with older databases, but all features may not be available.

## Supported Message Features

- Plain Text
  - Correctly extracts time-zone corrected timestamps
  - Detects when a message was read and calculates the time until read for both parties
    - Humanizes display of time-until-read duration
  - Parses `streamtyped` message body data
  - Detects the service a message was sent from
    - In HTML exports, balloons are colored correctly for the service they were sent with
- Edited and Unsent messages
  - Detects if messages were edited or unsent
    - Edited messages
      - Parses `streamtyped` message data
      - Displays content and timestamps for each edit
      - Humanizes display of edit timestamp gaps
      - Edited messages received before Ventura display as normal messages without history
    - Unsent messages
      - No content, but are noted in context
- Multi-part messages
  - iMessages can have multiple parts, separated by some special characters
  - Parts are displayed as
    - New lines in TXT exports
    - Separate balloons in HTML exports
- Threads and Message Replies
  - Threads are displayed both threaded under the parent as well as in-place
    - This is to preserve context, which can be lost if replying to older messages
    - Messages from a thread and were rendered in-place are annotated as such
  - For multi-part messages, replies are threaded under the correct message part
- Attachments
  - Any type of attachment that can be displayed on the web is embedded in the HTML exports
  - Attachments can be copied to the export directory or referenced in-place
  - Less-compatible HEIC images are converted to PNG for portable exports
  - Attachments are displayed as
    - File paths in TXT exports
    - Embeds in HTML exports (including `<img>`, `<video>`, and `<audio>`)
- Expressives
  - Detects both bubble and screen effects
  - Messages sent with expressives are annotated
- Reactions
  - Detects reactions to messages
  - Messages sent with reactions are annotated
  - For multi-part messages, reactions are placed under the correct message part
- Stickers
  - Detects stickers sent or placed on messages
  - Messages sent with stickers are
    - Displayed in HTML exports
    - Annotated in TXT exports
  - For multi-part messages, stickers are placed under the correct message part
- Apple Pay
  - Detects the transaction source, amount, and type
- URL Previews
  - Parses the `NSKeyedArchiver` payload to extract preview data
  - Extracts cached metadata for each URL
  - Preview images display in HTML exports
  - URLs that have rotten may still retain some context if they have cached data
- App Integrations
  - Parses the `NSKeyedArchiver` payload to extract balloon data
  - Supports system message types as well as third party applications
  - Supports Apple Music preview streams
  - Supports Rich Collaboration messages
  - Supports SharePlay/Facetime message balloons
- Duplicated group chats
  - Handles (participants) and chats (threads) can become duplicated
  - On startup:
    - Different handles that belong to the same person are combined
    - Chatrooms that contain identical contacts (i.e., duplicated handles) are combined

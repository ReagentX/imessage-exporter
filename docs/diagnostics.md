# Diagnostics

Diagnostic output from `imessage-exporter` looks like:

```txt
iMessage Database Diagnostics

Handle diagnostic data:
    Total handles: 552
    Handles with more than one ID: 2
    Total duplicated handles: 100
Message diagnostic data:
    Total messages: 183453
    Messages not associated with a chat: 43210
    Messages belonging to more than one chat: 36
    Recoverable messages: 1
    Date range: Sep 20, 2019  1:53:14 PM to Mar 09, 2026  5:03:14 PM
                6 years, 170 days, 4 hours, 58 minutes, 24 seconds
Attachment diagnostic data:
    Total attachments: 49422
        Data referenced in table: 44.13 GB
        Data present on disk: 31.31 GB
    Missing files: 15037 (30%)
        No path provided: 14929
        No file located: 108
Thread diagnostic data:
    Total chats: 432
    Total duplicated chats: 11
    Chats with no handles: 2
Global diagnostic data:
    Total database size: 339.88 MB
    Handles with resolved names: 231/452 (51%)
    Schema capabilities:
        Recoverable deleted messages: Detected
        Reply threads: Detected
        Tapbacks and poll votes: Detected
        Message filter categories: Detected

Environment Diagnostics

Detected converters:
    Image converter: sips
    Audio converter: afconvert
    Video converter: ffmpeg
```

## Handle diagnostic data

### Total handles

The total number of handles present in the provided iMessage database.

### Distinct `person_centric_id` values

The number of distinct `person_centric_id` values in the `handle` table. The iMessage server attached this merge ID to some incoming messages. It stopped issuing the ID for new senders long ago, so current databases include it only for the oldest contacts. We omit this line when the column is absent or empty.

### Handles merged into another handle

The number of `handle` rows absorbed into another row's group. We group rows when their `id` is byte-identical across services (the same phone number on iMessage, SMS, and RCS) or when they share a `person_centric_id`. During export, we count each group as one participant.

### Participants with more than one handle

The number of groups that contain more than one `handle` row after grouping. We show this line only when at least one handle merged.

## Message diagnostic data

### Total messages

The total number of rows in the `messages` table.

### Messages not associated with a chat

If a message exists in the `messages` table but does not have an entry in the `chat_message_join` table, it is considered orphaned and will be listed in either the `Orphaned.html` or `Orphaned.txt` file in the export directory. Likely, these come from messages that were deleted and the chat removed from the `chat_message_join` table, but the corresponding messages were not removed from the `messages` table.

### Messages belonging to more than one chat

If a message exists in the `messages` table and maps to multiple chats in `chat_message_join`, the message will exist in all of those chats when exported.

## Recoverable messages

The number of messages that were marked as deleted from conversations but are recoverable and will be rendered in-line in exports.

## Date range

The range of total available messages in the database in both exact and relative format.

## Attachment diagnostic data

### Total attachments

The total number of rows in the `attachments` table

#### Data referenced in table

The sum of the `total_bytes` column in the `attachments` table. I don't know why they are different, but the former is the actual storage taken up by iMessage attachments.

#### Data present on disk

Represents the total size of the attachments listed in the `attachments` when following the listed path to the respective file. Missing files may have been removed by the user or not properly downloaded from iCloud.

### Missing files

The first line shows the count and the percentage of files missing. In the example above, `15037 (30%)` means that `15,037` files (`30%` of the total number of attachments) are referenced in the table but do not exist.

There are two different types of missing files:

#### No path provided

This means there was a row in the `attachments` table that did not contain a path to a file.

#### No file located

This means there was a path provided, but there was no file at the specified location.

## Thread diagnostic data

### Total chats

The total number of chats present in the table.

### Total duplicated chats

The number of split group chats that have been coalesced together.

### Chats with no handles

Emits the count of chats that contain no chat participants.

## Global diagnostic data

### Total database size

The total size of the database file on the disk.

### Handles with resolved names

We group `handle` rows that belong to one person (see [the duplicate problem](tables/duplicates.md)), then report how many participant groups match a contact name and the match percentage. This total counts one participant per group, so it is smaller than the raw handle count.

### Handles merged by Contacts

The number of participant groups we eliminate with the Contacts database: handles with different addresses that the Messages database cannot relate, but that appear on one Contacts card. We show this line only when at least one group merged. The Messages-only counts in the handle section above do not include these.

### Chats merged by Contacts

The number of chats we eliminate because, once we group their participants by Contacts card, they have the same participants as another chat. We show this line with the one above.

### Schema capabilities

`imessage-database` probes the database schema once and builds its queries from what the schema actually supports, so databases missing optional features still read correctly. This section shows which optional features the reader detected.

#### Recoverable deleted messages

Whether the `chat_recoverable_message_join` table exists. When present, messages deleted within the last 30 days can be read and filtered.

#### Reply threads

Whether the `message.thread_originator_guid` column exists. When present, replies can be grouped under the messages they respond to.

#### Tapbacks and poll votes

Whether the `message.associated_message_guid` column exists. When present, tapbacks and poll votes can be resolved to the messages they target.

#### Message filter categories

Whether the `message.filter_action` and `message.filter_sub_action` columns exist. When present, the filter category each message was received under is read alongside the row.

## Detected converters

`imessage-exporter` uses third-party tools to convert images when using `--copy-method basic` or `--copy-method full`. This section shows what programs are detected on the current system.

### Image converter

The currently detected image converter, if present.

One of:

- `sips` (macOS Builtin)
- `magick` (`imagemagick`)
- None

### Audio converter

The currently detected audio converter, if present.

One of:

- `afconvert` (macOS Builtin)
- `ffmpeg`
- None

### Video converter

The currently detected video converter, if present.

One of:

- `ffmpeg`
- None

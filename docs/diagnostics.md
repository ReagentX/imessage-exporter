# Diagnostics

Diagnostic output from `imessage-exporter` looks like:

```txt
iMessage Database Diagnostics

Handle diagnostic data:
    Total handles: 609
    Distinct person_centric_id values: 2
    Handles merged into another handle: 104
    Participants with more than one handle: 88
Message diagnostic data:
    Total messages: 270778
    Messages not associated with a chat: 45778
    Messages belonging to more than one chat: 16
    Recoverable messages: 5
    Date range: Sep 20, 2019  1:53:14 PM to Oct 16, 2026  5:04:25 PM
                7 years, 26 days, 3 hours, 11 minutes, 11 seconds
Attachment diagnostic data:
    Total attachments: 75919
        Data referenced in table: 71.77 GB
        Data present on disk: 54.18 GB
    Missing files: 15668 (21%)
        No path provided: 15205
        No file located: 463
Thread diagnostic data:
    Total chats: 469
    Total duplicated chats: 20
Global diagnostic data:
    Total database size: 604.10 MB
    Handles with resolved names: 133/486 (27%)
    Handles merged by Contacts: 20
    Chats merged by Contacts: 20
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

The number of `handle` rows absorbed into another row's group. We group rows when their `id` is byte-identical across services (the same phone number on iMessage, SMS, and RCS) or when they share a `person_centric_id`. During export, we count each group as one participant. We show this line and the next only when at least one handle merged.

### Participants with more than one handle

The number of groups that contain more than one `handle` row after grouping.

## Message diagnostic data

### Total messages

The total number of rows in the `message` table.

### Messages not associated with a chat

If a message exists in the `message` table but does not have an entry in the `chat_message_join` table, it is considered orphaned and will be listed in either the `Orphaned.html` or `Orphaned.txt` file in the export directory. Likely, these come from messages that were deleted and the chat removed from the `chat_message_join` table, but the corresponding messages were not removed from the `message` table. We omit this line when the count is zero.

### Messages belonging to more than one chat

If a message exists in the `message` table and maps to multiple chats in `chat_message_join`, the message will exist in all of those chats when exported. We omit this line when the count is zero.

### Recoverable messages

The number of messages that were marked as deleted from conversations but are recoverable and will be rendered in-line in exports. We omit this line when the `chat_recoverable_message_join` table is absent; a count of zero still prints.

### Date range

The range of total available messages in the database in both exact and relative format. We omit this line when the `message` table is empty.

## Attachment diagnostic data

### Total attachments

The total number of rows in the `attachment` table.

#### Data referenced in table

The sum of the `total_bytes` column in the `attachment` table: the size every row claims, whether or not its file exists.

#### Data present on disk

The total size on disk of the files found by following each row's path. Missing files account for the gap between this figure and the one above; they may have been removed by the user or not downloaded from iCloud.

### Missing files

The first line shows the count and the percentage of files missing. In the example above, `15668 (21%)` means that `15,668` files (`21%` of the total number of attachments) are referenced in the table but do not exist. We omit this section when no files are missing.

There are two different types of missing files:

#### No path provided

This means there was a row in the `attachment` table that did not contain a path to a file.

#### No file located

This means there was a path provided, but there was no file at the specified location.

## Thread diagnostic data

### Total chats

The number of chats with at least one row in `chat_handle_join`. Chats without participants are excluded here and counted below.

### Total duplicated chats

The number of chat rows removed by deduplication. We group each chat's handles as described in [the duplicate problem](tables/duplicates.md), then merge chats that `chat_lookup` links, named chats whose participant groups and names match, and unnamed chats whose participant groups match. This count groups handles from Messages data alone; merges that depend on Contacts appear under [Chats merged by Contacts](#chats-merged-by-contacts). We omit this line when the count is zero.

### Chats with no handles

The number of chats referenced in `chat_message_join` that have no rows in `chat_handle_join`. We omit this line when the count is zero.

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

`imessage-exporter` uses external programs to convert attachments: `--copy-method basic` converts images, and `--copy-method full` also converts audio and video. This section shows which programs are present on the current system.

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

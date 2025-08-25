# Diagnostics

Diagnostic output from `imessage-exporter` looks like:

```txt
iMessage Database Diagnostics

Handle diagnostic data:
    Contacts with more than one ID: 2
Message diagnostic data:
    Total messages: 183453
    Messages not associated with a chat: 43210
    Messages belonging to more than one chat: 36
Attachment diagnostic data:
    Total attachments: 49422
        Data referenced in table: 44.13 GB
        Data present on disk: 31.31 GB
    Missing files: 15037 (30%)
        No path provided: 14929
        No file located: 108
Thread diagnostic data:
    Chats with no handles: 2
Global diagnostic data:
    Total database size: 339.88 MB
    Duplicated contacts: 78
    Duplicated chats: 16

Environment Diagnostics

Detected converters:
    Image converter: sips
    Audio converter: afconvert
    Video converter: ffmpeg
```

## Handle diagnostic data

### Contacts with more than one ID

The number of contacts that have multiple entries in the `handle` table, deduplicated by matching their `person_centric_id` across rows. The `person_centric_id` is a field used by Apple to disambiguate contacts. Further deduplication also happens, as noted below.

## Message diagnostic data

### Total messages

The total number of rows in the `messages` table.

### Messages not associated with a chat

If a message exists in the `messages` table but does not have an entry in the `chat_message_join` table, it is considered orphaned and will be listed in either the `Orphaned.html` or `Orphaned.txt` file in the export directory. Likely, these come from messages that were deleted and the chat removed from the `chat_message_join` table, but the corresponding messages were not removed from the `messages` table.

### Messages belonging to more than one chat

If a message exists in the `messages` table and maps to multiple chats in `chat_message_join`, the message will exist in all of those chats when exported.

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

Emits the count of chats that contain no chat participants.

## Global diagnostic data

### Total database size

The total size of the database file on the disk.

### Duplicated contacts

Duplicated contacts occur when a single contact has multiple valid phone numbers or iMessage email addresses. The iMessage database stores handles as rows, and multiple rows can match to the same contact.

### Duplicated chats

The number of separate chats that contain the same participants. See the [duplicates](/docs/tables/duplicates.md) for a detailed explanation of the logic used to determine this number.

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

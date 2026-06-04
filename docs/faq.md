# Frequently Asked Questions

## I cannot connect to the messages database. What do I do?

Ensure your terminal emulator has [full disk access](https://kb.synology.com/en-us/C2/tutorial/How_to_enable_Full_Disk_Access_on_a_Mac) if using the default location or ensure that the path to the database file is correct. You can enable this in System Settings > Privacy & Security > Full Disk Access.

***

## Are emojis, tapbacks (reactions), and other special message features preserved in the export?

Yes, all iMessage features are supported. See [here](features.md) for more detail.

***

## Can it export messages from third-party apps that integrate with iMessage?

Yes. See [here](features.md) for more detail on supported features.

***

## Does `imessage-exporter` export message conversations that are in iCloud or on a user's iPhone/iPad but not on the user's Mac?

`imessage-exporter` only reads data present in the provided source, which can be a macOS `chat.db`, a local iOS backup (encrypted or unencrypted), or a jailbroken iOS `sms.db`. It cannot read data that is only stored in iCloud.

***

## Can I force iCloud to download attachments that were offloaded?

In the Messages app, if you click the info (`ⓘ`) button for a conversation and scroll to the bottom, there is a button that downloads all of the attachments for that conversation. This works on both macOS and iOS.

![](../docs/binary/img/icloud_download.png)

## Can it export group conversations as well as individual chats?

Yes.

***

## Can I export only specific conversations?

Yes, the `--conversation-filter` (`-t`) argument filters by contact name, phone number, or email. Multiple filters can be comma-separated, e.g., `-t "steve@apple.com,5558675309"`. Substring matching is supported, so `-t "+1555"` matches all numbers with that prefix. All conversations containing the matched participants are included.

See [here](../imessage-exporter/README.md#how-to-use) for details on `imessage-exporter` arguments.

***

## How does the exporter handle previously exported messages?

If files with the current output type exist in the output directory, `imessage-exporter` will alert the user and the export will not start. If the export directory is clear, the command-line binary exports all messages by default. The desktop GUI adds a safeguard: if no conversations are selected, it asks for confirmation and shows the total number of conversations and messages before exporting everything. Alternatively, exports can be narrowed with selected conversations, participant filters, or the `--start-date` and `--end-date` arguments.

See [here](../imessage-exporter/README.md#how-to-use) for details on `imessage-exporter` arguments.

***

## Can I cancel an export after it starts?

Yes. In the desktop GUI, press **Cancel export** while an export is running. The exporter stops at safe checkpoints and reports that the export was cancelled. The command-line binary is intended for headless use; stop it with your shell or process supervisor if needed.

***

## Is it possible to export a conversation and re-integrate it back onto another Apple ID?

No, I do not want to be trusted with write access to your iMessage data. This software is *read only*.

***

## Is there a search function?

No, this software just builds exports. I use [`ripgrep`](https://github.com/BurntSushi/ripgrep) to search though the exported files.

***

## Can it export messages between a specific date range?

Yes, the `--start-date` and `--end-date` arguments specify date ranges for exports.

See [here](../imessage-exporter/README.md#how-to-use) for details on `imessage-exporter` arguments.

***

## Are voice messages saved?

Expired ones cannot be because they are deleted from disk by the system. If you kept them, they are included in the exports.

***

## Are messages deleted from the messages app erased from the database?

This software can recover some, but not all, deleted messages.

Messages removed by deleting an entire conversation or by deleting a single message from a conversation are moved to a separate collection for up to 30 days. Messages present in this collection are restored to the conversations they belong to. Apple details this process [here](https://support.apple.com/en-us/HT202549#delete).

Messages that have expired from this restoration process are permanently deleted and cannot be recovered.

In some instances, deleted messages are removed from the `chat_message_join` table but not from the `messages` table. These messages will populate in `orphaned.html`, `orphaned.txt`, or `orphaned.pdf`, depending on the export format.

***

## What is the `orphaned` file in my export?

Messages that cannot be associated with any conversation are written to `orphaned.html`, `orphaned.txt`, or `orphaned.pdf`, depending on the export format. This can happen when a message's chat has been deleted from the `chat_message_join` table, or when the database has inconsistencies. These messages are preserved so no data is lost.

***

## What export formats are supported? Can I export to PDF?

`imessage-exporter` supports native `txt`, `html`, and `pdf` conversation exports. PDF export is generated directly by the app, without Safari or a browser-based print workflow. You can still use `--no-lazy` with HTML exports if you prefer to print HTML from a browser yourself.

***

## Can it export call logs?

Yes, from iOS backup folders when Apple's call-history database is present. Use `--call-logs -p <backup folder> -a iOS -o <export folder>` to write `call_logs.csv`.

Recent unencrypted iOS backups may omit call history. If the database is missing, the app reports the exact backup manifest paths it tried.

***

## How do I customize the appearance of HTML exports?

Each HTML export file links to an external `style.css` in the export directory. You can create this file to override the default styles. Since custom styles load after the embedded defaults, they take precedence at the same specificity.

***

## What does `--copy-method` do and which should I choose?

- `disabled` (default): Attachments are not copied; the export references them in-place by filesystem path
- `clone`: Copies all attachment files as-is, preserving original formats and quality
- `basic`: Copies all files, converting `HEIC` images to `JPEG` for broader compatibility
- `full`: Copies all files, converting `HEIC` to `JPEG`, `CAF`/`AMR` audio to `MP4`, `MOV` video to `MP4`, and animated sticker `HEICS` to `GIF`

`basic` and `full` require external tools (`sips` or ImageMagick for images, `afconvert` or `ffmpeg` for audio, `ffmpeg` for video). `disabled` and `clone` do not require or probe media converters. The `--diagnostics` output shows which converters are detected on your system.

***

## What does the `--diagnostics` flag show?

Diagnostics mode prints a summary of the database without exporting anything:

- Handle data: total handles, duplicated handles
- Message data: total messages, orphaned messages, messages in multiple chats, recoverable deleted messages, and the date range of all messages
- Attachment data: total attachments, referenced vs. on-disk data sizes, and missing file counts
- Thread data: total chats, duplicated chats, chats with no handles
- Global data: database file size, percentage of handles with resolved contact names, and detected media converters

***

## What does `--attachment-root` do?

This option overrides where the app looks for attachment files. It is useful when attachment data is stored separately from the database, such as on an external drive. The provided path replaces the default `~/Library/Messages` root and affects both the `Attachments` and `StickerCache` directories. It also works with jailbroken iOS `sms.db` databases (use `--platform macOS`). This option has no effect on iOS backups.

***

## Why do I see "Unable to build contacts index"?

This warning means the Contacts database could not be read. The export will continue, but participants will be labeled with phone numbers and email addresses instead of resolved contact names. This commonly happens due to missing Full Disk Access permissions, since the contacts database is also protected. You can also specify a custom contacts database path with `--contacts-path`.

***

## How fast is `imessage-exporter`?

This is a complicated question that depends on CPU, database size, chosen export type, encryption state, and chosen attachment handling style.

On my M1 Max MacBook Pro, approximate performance is as follows:

| `--copy-method` | Messages exported per second |
|---|---|
| `disabled` | > 145,000 |
| `clone` | ≈ 42,000 |
| `basic` | ≈ 350 |
| `full` | ≈ 250 |

For more information on `--copy-method`, see [here](../imessage-exporter/README.md#how-to-use) and [here](./features.md#supported-message-features).

However, if you recently deleted a large amount of data from Messages, the database will be slow for awhile, resulting in significantly reduced performance from `imessage-exporter`.

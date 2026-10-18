# Diagnostics and changed semantics

## Handle diagnostic fields

Update code that reads or constructs `HandleDiagnostic`:

| 4.3.0 field | 5.0 field | Meaning in 5.0 |
| --- | --- | --- |
| `total_handles` | `total_handles` | Actual handle rows, excluding the synthetic local-user row `0` |
| `handles_with_multiple_ids` | `person_centric_ids` | Distinct non-empty `person_centric_id` values; `None` when the column is absent |
| `total_duplicated` | `merged_handles` | Handle rows absorbed into another row's group |
| No equivalent | `multi_handle_groups` | Participant groups containing more than one handle row |

These counts describe different things. A group of three handle rows contributes two to `merged_handles` and one to `multi_handle_groups`. The `person_centric_ids` count describes the stored merge IDs, regardless of how many rows each identifies.

`Handle::run_diagnostic()` uses Messages data alone. The exporter reports additional Contacts-based merges in its global diagnostics. Its name-resolution percentage now counts grouped participants, so its denominator differs from the raw handle count. Scripts that parse the diagnostic text must also update the renamed labels and account for lines omitted when no merges occur. See [diagnostics](../../diagnostics.md) for the output fields.

## Dates

Decode timestamps to `DateTime<Utc>` without passing an epoch offset. Replace `util::dates::get_offset()` with the constant `util::dates::MESSAGES_EPOCH_UNIX_SECS`, and migrate `get_local_time()` calls as shown below.

| 4.3.0 | 5.0 |
| --- | --- |
| `get_local_time(stamp, offset)` | `to_utc(stamp)?.with_timezone(&Local)` |
| `message.date(offset)` and the other three date accessors | `message.date()?.with_timezone(&Local)` |
| `message.time_until_read(offset)` | `message.time_until_read(&Local)` |
| `balloon.check_in_kind(0)` | `balloon.check_in_kind()`, then `with_timezone()` on the returned `DateTime<Utc>` |
| `context.set_start(date)` / `set_end(date)` | `context.set_start(date, &Local)` / `set_end(date, &Local)` |

Pass `&DateTime<Tz>` to `format()` and `readable_diff()` for any `Tz: TimeZone`. Use the same zone for both arguments of `readable_diff()`. For file timestamps or sorting, use the `DateTime<Utc>` directly, without `with_timezone()`. Replace `Local` with any other `TimeZone` to render in a different zone.

## Poll vote updates

`Poll::count_votes()` retains its signature, but now applies each payload as the current selections for the voters it names. It removes those voters' previous selections, deduplicates repeated votes, and applies the new selections. Votes belonging to other participants remain intact. `Poll::apply_votes()` exposes the same operation for an already parsed `Vec<PollVote>`.

Apply updates in their intended order. An empty vote list names no voters, so it leaves the poll unchanged; it does not retract every vote. Update expectations that previously counted repeated snapshots as additional votes.

## Archived null values

`parse_ns_keyed_archiver()` omits references to the archiver's null placeholder from dictionaries and arrays. A null root returns `PlistParseError::NoPayload`. Update consumers that expected missing fields or array elements to appear as the string `"$null"`.

Literal strings containing `"$null"` remain strings. The parser identifies the archive's null object by its object-table position as well as its contents.

## Database path errors

Handle `TableError::CannotConnect` from `Platform::determine()` for a missing or inaccessible path, or a directory without a Messages database. If you previously probed a path you knew was absent, handle the error instead of expecting `Platform::macOS`.

Update your `TableConnectError` match arms for the 5.0 variants:

| 4.3.0 variant | 5.0 variant |
| --- | --- |
| `NotAFile(PathBuf)` | `IsDirectory(PathBuf, Vec<PathHint>)` |
| `DoesNotExist(PathBuf)` | `DoesNotExist(PathBuf, Vec<PathHint>)` |
| No equivalent | `Inaccessible(PathBuf, std::io::Error)` for a filesystem error when inspecting or opening a path |
| No equivalent | `NotADatabase(PathBuf)` for a file without the SQLite header |
| No equivalent | `NotMessagesDatabase(PathBuf)` from `Capabilities::determine()` when the `message` table is absent |

Use `util::path_hint::PathHint` to identify literal quote characters, backups found on disk, a `chat.db` inside the directory, a backup root without a Messages database, a pre-iOS 10 flat layout, or the first missing path component. Call `util::path_hint::diagnose()` to collect the hints and `util::path_hint::presence()` to distinguish a missing path from a permission error. Call `util::dirs::default_backup_roots()` to locate the host's default Finder, iTunes, and Apple Devices backup directories.

## Removed and moved items

`Announcement::Unknown`, `MessageError::TypedStreamError`, and `PlistParseError::StreamTypedError` are removed, along with the `From` conversions into the two error variants. Remove any match arms for them; none of these variants was ever produced, so no replacement is needed.

`tables::table` no longer defines `YOU`, `UNKNOWN`, `ORPHANED`, `ATTACHMENTS_DIR`, or `ATTRIBUTED_BODY`. Define any you use in your own crate; the library does not read them. `ME`, `FITNESS_RECEIVER`, the table and column names, and the default database paths remain.

`util::dirs::home()` is removed; call `std::env::home_dir()`, which reads `HOME` on Unix and `USERPROFILE` on Windows. `util::dirs::default_db_path()` returns `Option<PathBuf>` and is `None` when no home directory is known. `util::dirs::expand_home()` replaces a leading `~` in a stored attachment path with the home directory.

The modules `tables::messages::message`, `message_types::digital_touch::models`, and `message_types::handwriting::models` are private. Import `Message` from `tables::messages`, `DigitalTouchMessage`, `Color`, `ImageBackdrop`, and `Point` from `message_types::digital_touch`, and `HandwrittenMessage` and `Point` from `message_types::handwriting`. The Digital Touch `decode_*` helpers and the `util::typedstream` module (`TypeLengthPair`, `as_type_length_pair`) are no longer public.

Omit `copied_path` when constructing an `Attachment`; remove `copied_path: None` from struct literals. After `from_row()`, expect no further library writes to `mime_type`. When copying or converting attachment files, keep the output path and, for a conversion, the output media type in a separate value beside the row. Read `mime_type()` for the source type and your value for the exported type.

`HandwritingError` gains `InvalidDimensions(u16, u16)`, returned by `HandwrittenMessage::from_payload()` for a frame whose width or height cannot be padded without overflowing.

`imessage_database` re-exports `chrono`, `plist`, and `rusqlite`. Import their types through the re-exports, for example `imessage_database::rusqlite::Connection`, and remove your own dependency on each crate unless other code needs it.

## CLI and export changes

Existing export commands remain usable. When Contacts provides an unambiguous relationship between a person's addresses, the exporter can combine conversations previously written separately. Participant grouping, representative selection, and filename truncation can also change transcript names and attachment directory names.

Use a fresh output directory when comparing a 4.3.0 export with a 5.0 export. Compare conversation contents as well as file counts: one merged transcript may replace several older files. Rebuild any external index that depends on generated filenames or computed conversation IDs.

`--contacts-path` (`-n`) now overrides the Contacts database in both encrypted and unencrypted iOS backups. Remove the option if you previously supplied it for an iOS backup and want to keep using the backup's Contacts data; earlier versions ignored that override.

Conversation filters match every Messages address in a grouped participant, so filtering by one address can select messages associated with another address in the same group. An unmatched conversation filter now returns an error even when date filters are present. Update automation that relied on the date filter allowing that invocation to proceed.

[Back to the migration guide](README.md).

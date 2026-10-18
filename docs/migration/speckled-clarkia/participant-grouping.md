# Participant grouping and chat deduplication

`Handle::dedupe()` is removed. Build `HandleGroups` from handle rows, add any external relationships, then call `finalize()` to obtain the map from handle row ID to participant group ID.

`HandleGroups::from_rows()` and `from_db()` join identical non-empty addresses and matching non-empty `person_centric_id` values. They do not read Contacts. The exporter supplies Contacts relationships separately; library callers can add their own relationships with `union(handle_row_id, handle_row_id)` before finalizing. Unknown row IDs have no effect.

`ChatToHandle::dedupe()` now takes four arguments and returns the map directly instead of a `Result`; remove the `?` or `unwrap()` on the call:

```rust,ignore
ChatToHandle::dedupe(&participants_by_chat, &chat_lookup, &handle_groups, &chats);
```

Load handle groups before deduplicating chats, because chat comparisons now use participant groups. The following example accepts externally resolved pairs of handle row IDs and returns the representative chat row ID for each merged conversation's member rows:

```rust,no_run
use std::collections::HashMap;

use imessage_database::{
    error::table::TableError,
    tables::{
        chat::Chat,
        chat_handle::ChatToHandle,
        handle::{Handle, HandleGroups},
    },
};
use rusqlite::Connection;

fn conversation_representatives(
    db: &Connection,
    contact_links: &[(i32, i32)],
) -> Result<HashMap<i32, i32>, TableError> {
    let handles = Handle::load(db)?;
    let mut groups = HandleGroups::from_rows(&handles);
    for &(left, right) in contact_links {
        groups.union(left, right);
    }
    let handle_groups = groups.finalize();

    let chats = Chat::by_rowid(db)?;
    let participants_by_chat = ChatToHandle::participants_by_chat(db)?;
    let chat_lookup = ChatToHandle::get_chat_lookup_map(db)?;
    let merged_chats =
        ChatToHandle::dedupe(&participants_by_chat, &chat_lookup, &handle_groups, &chats);
    let last_activity = ChatToHandle::get_last_activity(db)?;

    Ok(ChatToHandle::representatives(
        &merged_chats,
        &chats,
        &last_activity,
        &participants_by_chat,
    ))
}
```

Pass an empty slice when you have no external links. If you also need the raw address map, call `Handle::ids_by_rowid(handles)` after borrowing the rows for `HandleGroups::from_rows()`. This lets both operations share one table load.

Handle row IDs, participant group IDs, deduplicated chat IDs, and representative chat row IDs have different meanings, even though each uses `i32`. A representative map is keyed by original chat row ID and returns another chat row ID; a deduplication map returns a computed group ID. Computed group IDs are deterministic for unchanged inputs, but can change when the data or supplied relationships change. Rebuild the maps rather than persisting those IDs as database identities.

To merge chats:

- Merge conversations connected through `chat_lookup`, even with different display names.
- Otherwise, merge named chats only with matching participant groups and display names.
- Attach an unnamed chat to the lowest-row-ID named chat with the same participant groups, unless you already grouped it with a named chat through `chat_lookup`.
- For an unnamed chat linked to a named conversation, use that conversation when attaching other unnamed chats with matching participant groups. Preserve existing assignments to other named conversations from `chat_lookup`.
- Without a matching named chat, merge unnamed chats with the same participant groups.
- Assign a deduplicated ID to every chat referenced in `chat_lookup`, including chats without handle rows, to route their messages to the conversation. Assign a separate ID to each conversation made entirely of such chats.

Before generating filenames for a custom exporter, choose one representative row per merged conversation. Call `representatives()` with the participant map to apply these rules:

1. Use the most recently active row if named.
2. Otherwise, use the named row with the lowest ID.
3. For an entirely unnamed conversation, use the most recently active row with participant rows.
4. For an unnamed conversation without participant rows, use the most recently active row.

Break activity ties by lowest row ID. Prefer rows with a name or participants for building filenames; use an unnamed lookup-only row only for a conversation without either. Use the representative for the filename, and route transcripts and attachments through the same deduplicated conversation ID. See [the duplicate problem](../../tables/duplicates.md) for the grouping mechanism.

[Back to the migration guide](README.md).

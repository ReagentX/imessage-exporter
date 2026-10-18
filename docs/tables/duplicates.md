# The Duplicate Problem

iMessage has had a long history of splitting both one-on-one and group chats into separate chats. This creates a problem for this tool: in order to iterate over the table of Messages to export them, the tool has to understand where to sort them.

## Types of Duplicated Data

Both handles (participants) and chats (threads) can become duplicated, where handle duplication leads to chat duplication.

### Duplicate Handles

We store one `(id, service)` pair per `handle` row, so one person may have several rows. For example, if you register a phone number with iMessage, SMS, and RCS, we store three rows with the same `id`. We also store a phone number and an iCloud email address in separate rows with different IDs. You cannot use the `handle` table alone to connect these addresses to your Contacts cards. At read time, Messages.app checks `person_centric_id`, then the Contacts card associated with the address, then the raw `id` to resolve the person. We make the same three checks.

The iMessage server attached `Person Centric ID` as a merge ID to incoming messages and used to provide it for nearly every contact. The server stopped issuing it for new senders years ago; today, the field appears only on the oldest handles in current databases. We still trust a present ID because it is reliable, but we cannot rely on it to cover current contacts.

### Duplicate Conversations

You may see separate `Chat ID`s when you reach the same person through different addresses. For example, if you contact one group member by phone one day and by email the next, you may see two group chats. We can merge some chats split across services by following `chat_lookup`, but we cannot use it to identify splits caused by different handles.

## Sample Data

These sample tables help visualize the problem.

### Handles Table

Contact `A` has an email `Y` and a phone number `X`, and `X` is registered on two services. Contact `B` has a single phone number `Z`.

| Handle ID | Contact | Service | Person Centric ID |
| -- | -- | -- | -- |
| 1 | Y | iMessage | A |
| 2 | X | iMessage | A |
| 3 | X | SMS | |
| 4 | Z | iMessage | |

We group handles `1` and `2` with `A` because they share a `Person Centric ID`. Handle `3` lacks that value, but we still group it with `A` because its `id` matches handle `2`. If `A` later writes from a second email address absent from this database, we cannot tell from the database alone that it belongs to `A`; only the Contacts app can supply that link.

### Chats Table

The table below lists the handle rows that appear in each chat (thread):

| Chat ID | Handle ID |
| -- | -- |
| 10 | 1 |
| 11 | 2 |
| 12 | 2 |
| 12 | 4 |
| 13 | 3 |
| 13 | 4 |

Note: The `Chat ID` is not unique, it represents group chats as well, so in this example there is a group chat `12` with 3 participants, yourself, `A`, and `B`.

## The Many-To-Many Problem

You should see one conversation when `A` writes from either `Y` or `X`, since both addresses belong to `A`. You should also see one group chat for messages from all addresses used by its participants. You can identify an address by its `Handle ID`, but that field alone cannot tell you who owns it. You may therefore see separate participants when one person uses different addresses.

If `A` writes from both `Y` and `X`, you will see two conversations. In the table above, you see messages from `Y` under chat `10` and messages from `X` under chat `11`, even though both addresses belong to `A` and should appear in one conversation.

When `A` writes to you and `B` from `X` over SMS, you see those messages under chat `13`. Chat `13` has the same participants as chat `12`, but you reach `A` through a different `Handle ID`. As a result, you see those messages in a separate chat even though both chats have the same participant groups.

## Solving the Problems

### Handles Solution

We need one identifier for every row that belongs to the same person, including rows connected through a third row. To assign these identifiers, we seed a union-find with the `Handle ID`s and add an edge whenever two rows provide evidence that they belong to one person. We get two kinds of evidence from the database:

1. Matching `id` values, byte for byte, as with handles `2` and `3`. Messages already lowercases email addresses and canonicalizes phone numbers before storing them, so we do not need to normalize these values again. If we match phone numbers by digits alone, we could merge a short code with a full number that happens to share its suffix.
2. Matching `Person Centric ID` values, as with handles `1` and `2`.

We add a third kind of evidence from the Contacts database:

3. Addresses that appear on one Contacts card. We match phone numbers as written: a leading `+` claims a country code, a number without one may carry any, and a `+1` number also matches its bare ten digits. We treat cards linked through `PersonLink` on iOS or `ZLINKID` on macOS as one card. We skip the "me" card, and we add no edge for an address that appears on two different cards, since we cannot tell which person it belongs to.

Because group membership is transitive, we can group handle `3` with `A` through handle `2`, even though `3` has no `Person Centric ID`. We assign sequential IDs by each group's smallest `Handle ID` to keep the map stable across runs while the table remains unchanged. For these rows, assign the following group IDs:

```json
{
    1: 0,
    2: 0,
    3: 0,
    4: 1
}
```

Use the map to resolve each `Handle ID` to its person group. We omit row `0` (you in group chats) from the sample. In practice, assign group `0` to row `0`, then add one to every group ID shown above.

We read the Contacts database on every run and never cache its identities. When you merge two cards in Contacts, the app deletes one of them, so an identity we compute on one run may not exist on the next.

### Chats Solution

We use handle groups as per-person identifiers when comparing chats; the chats table has no such identifier. After grouping handles, we build a second union-find over the `Chat ID`s and add edges from two kinds of evidence:

1. Follow `chat_lookup` links transitively to group related chats.
2. Compare participant sets after replacing each `Handle ID` with its group ID. For matching sets, merge named rows only with matching display names. Attach each unnamed row to the lowest-ID named row with the same participants, unless you already grouped it with a named row through `chat_lookup`. For an unnamed row linked to a named conversation, use that conversation when attaching other unnamed rows with matching participants. Preserve existing assignments to other named conversations from `chat_lookup`. Without a matching named row, merge the unnamed rows together.

Assign an ID to every row referenced in `chat_lookup`, including rows without handle rows. Assign a separate ID to each conversation made entirely of such rows.

For the sample rows, chats `10` and `11` both contain participant group `{0}`, while chats `12` and `13` both contain groups `{0, 1}`. We assign each resulting set a sequential unique chat ID, using the same ordering as for handle groups. During export, we use the `Chat ID` to unique chat ID map to place messages in the right conversations. We also use that unique ID to name each chat's attachment directory, keeping each transcript and its attachments together.

We store `display_name` per chat row, so we can record different names within a merged conversation. You may have unnamed rows from before you named the group. Choose one representative row for each conversation's filename:

1. Use the most recently active row if named.
2. Otherwise, use the named row with the lowest ID.
3. For an entirely unnamed conversation, use the most recently active row with handle rows.
4. For an unnamed conversation without handle rows, use the most recently active row.

Prefer rows with a name or participants for building filenames; use an unnamed lookup-only row only for a conversation without either. When viewing a message in Messages.app, you see the name stored on its chat row. For a named conversation, use the representative row's name in the filename to match the name you see in the app.

When no row in a conversation has a display name, we build its filename from one name per participant group rather than one name per handle row. We use that filename for every chat in the merged conversation.

# The Duplicate Problem

iMessage has had a long history of splitting both one-on-one and group chats into separate chats. This creates a problem for this tool: in order to iterate over the table of Messages to export them, the tool has to understand where to sort them.

## Types of Duplicated Data

Both handles (participants) and chats (threads) can become duplicated, where handle duplication leads to chat duplication.

### Duplicate Handles

Handles (contacts) are often duplicated, usually when a contact sends messages from a phone number and an iCloud account. As a result. the phone number and the email address for the same contact may exist under different `Handle ID`s.

### Duplicate Conversations

Because of the aforementioned handle problem, some conversations with the same handle are mapped into separate `Chat ID`s.

## Sample Data

These sample tables help visualize the problem.

### Handles Table

Contact `A` has an email `Y` and a phone number `X`. Each are listed in the table under a different `Handle ID`. Contact `B` has a single phone number `Z`. The iMessage database makes no connection to your Contacts app here, it just knows these items exist.

| Handle ID | Contact | Person Centric ID |
| -- | -- | -- |
| 1 | Y | A |
| 2 | X | A |
| 3 | Z | B |

The `Person Centric ID` is a unique identifier for each contact. Here, we can use it to determine which `Handle`s belong to `A` and which belong to `B`.

### Chats Table

When you receive a message from `A`, you either receive from the address `Y` or `Z`. Each of these is listed under a separate chat (thread):

| Chat ID | Handle ID | Group ID |
| -- | -- | -- |
| 10 | 1 | A |
| 11 | 2 | A |
| 12 | 3 | B |
| 12 | 2 | B |

Note: The `Chat ID` is not unique, it represents group chats as well, so in this example there is a group chat `12` with 3 participants, yourself, `A`, and `B`.

## The Many-To-Many Problem

Chats between yourself and `A` *should* go to the same chat, because `Y` and `X` are actually the same person. Further, any messages sent to chat `12` from any of the addresses of the participants *should* go to the same chat, not a separate one with the same participants. This does not happen, because the iMessage Database uses the `Handle ID` field here, not the `Person Centric ID`.

If contact `A` sends messages from `Y` and `X`, you will see two separate conversations. Given the above table, messages from `Y` will go to `10` and messages from `X` will go to `11`, even though said messages should belong to the same chat.

Further, if contact `A` sends messages from `Y` and `X` to you and `B`, only messages from `X` will go to chat `12`. Messages `A` sends from `Y` will go to a new chat, `13`, that contains the same participants but with `A` under a second ID:

| Chat ID | Handle ID |
| -- | -- |
| 10 | 1 |
| 11 | 2 |
| 12 | 3 |
| 12 | 2 |
| 13 | 1 |
| 13 | 3 |

Chats `12` and `13` are the same group of people, but because the IDs of the participants are different, the messages get sorted into a separate chat.

## Solving the Problems

### Handles Solution

Applying a small two-pass check solves this problem, since the handles table only contains a few columns of metadata.

First, generate a map of each `Person Centric ID` to its corresponding handles. The second pass iterates over each handle in the table, building a hashmap of `Handle ID` to a string that combines all of the corresponding handles' metadata. For example, for the above table, we would generate a hashmap that looks like

```json
{
    1: "X, Y",
    2: "X, Y",
    3: "Z"
}
```

Now, any lookups against a contact ID will result in the same metadata.

### Chats Solution

`Person Centric ID` gives us a unique field that exists on a per-contact basis. In the above handles table, all of the handles associated with `A` have the same `Person Centric ID`. If we want to map our conversations against this, we need to fix this many-to-many problem. We can solve it with a few hash tables, with the second containing a new field:

1. Receive a reference to the hashmap generated for the `Handle` table
2. Participants to Unique Chat ID: `Set(Person Centric ID)` -> Set(`Chat ID`)
3. Chat to Unique Chat: `Chat ID` -> `Unique Chat ID`

The steps to generate this unique chat identifier are as follows:

- Generate hashmap `1` that contains all of the sets of participants
- Generate hashmap `2` by iterating through the values of `1`, inserting each new set of participants
- Generate hashmap `3` by inverting hashmap `2`

From here, when we iterate over messages, we can use the `Chat to Unique Chat` table to organize our messages into the proper chats.

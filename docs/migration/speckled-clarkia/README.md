# Migrating to 5.0: Speckled Clarkia

[Speckled Clarkia](https://calscape.org/plant/1034/Clarkia-cylindrica-ssp.-cylindrica-(Speckled-Clarkia)) is the 5.0 release of iMessage Exporter. This guide covers the changes from 4.3.0, with particular attention to callers of `imessage-database`.

## Message body decoding

Read decoded body fields directly and replace the separate body-parsing calls.

[Read message body decoding](message-bodies.md).

## Row iteration

Replace callback streaming and update code that uses the table traits.

[Read row iteration](row-iteration.md).

## Caches and blob access

Replace cache builders and database helpers with their concrete APIs.

[Read caches and blob access](caches-and-blobs.md).

## Custom message projections

Select the inputs needed for body decoding, including the storage bytes in UTF-16 databases.

[Read custom message projections](message-projections.md).

## Participant grouping and chat deduplication

Build participant groups before merging chats, then choose consistent conversation filenames.

[Read participant grouping and chat deduplication](participant-grouping.md).

## Diagnostics and changed semantics

Update diagnostic consumers, date decoding, poll and archive handling, and CLI automation that depends on export output.

[Read diagnostics and changed semantics](diagnostics-and-behavior.md).

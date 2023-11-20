# Frequently Asked Questions

#### I cannot connect to the messages database. What do I do?

Ensure your terminal emulator has [full disk access](https://kb.synology.com/en-us/C2/tutorial/How_to_enable_Full_Disk_Access_on_a_Mac) if using the default location or ensure that the path to the database file is correct.

***

#### Does `imessage-exporter` export message conversations that are on a user's iPhone/iPad but not on the user's Mac?

No, `imessage-exporter` only reads data present on the host system.

***

#### How does the exporter handle previously exported messages?

If files with the current output type exist in the output directory, `imessage-exporter` will alert the user that they will overwrite existing exported data and the export will be cancelled. If the export directory is clear, `imessage-exporter` will export all messages by default, or between the dates specified by the `--start-date` and `--end-date` arguments.

See [here](../imessage-exporter/README.md#how-to-use) for details on `imessage-exporter` arguments.

***

#### Is it possible to export a conversation and re-integrate it back onto another Apple ID?

No, I do not want to be trusted with write access to your iMessage data. This software is *read only*.

***

#### Is there a search function?

No, this software just builds exports. I use [`ripgrep`](https://github.com/BurntSushi/ripgrep) to search though the exported files.

***

#### Will it run on Windows/Linux?

I don't pre-build binaries for Windows or Linux, but it should compile to those [targets](https://doc.rust-lang.org/nightly/rustc/platform-support.html). As long as you can point it at an iMessage database, it should work.

***

#### Are voice messages be saved?

Expired ones cannot because they are deleted. If you kept them then they are included in the exports.

***

#### Are messages deleted from the messages app erased from the database?

This software can recover some, but not all, deleted messages.

Messages removed by deleting an entire conversation or by deleting a single message from a conversation are moved to a separate collection for up to 30 days. Messages present in this collection are restored to the conversations they belong to. Apple details this process [here](https://support.apple.com/en-us/HT202549#delete).

Messages that have expired from this restoration process are permanently deleted and cannot be recovered.

***

#### How fast is `imessage-exporter`?

This is a complicated question that depends on CPU, database size, chosen export type, and chosen attachment handling style.

On my M1 Max MacBook Pro, performance is as follows:

- With `--copy-method disabled`, exports run at about `18k` messages per second
- With `--copy-method efficient`, exports run at about `13k` messages per second
- With `--copy-method compatible`, exports run at about `300` messages per second
- For more information on `--copy-method`, see [here](../imessage-exporter/README.md#how-to-use) and [here](./features.md#supported-message-features).

However, if you recently deleted a large amount of data from Messages, the database will be slow for awhile, and will result in significantly reduced performance from `imessage-exporter`.

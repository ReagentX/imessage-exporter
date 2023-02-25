# Frequently Asked Questions

#### I cannot connect to the messages database. What do I do?

Ensure your terminal emulator has [full disk access](https://kb.synology.com/en-us/C2/tutorial/How_to_enable_Full_Disk_Access_on_a_Mac) if using the default location or ensure that the path to the database file is correct.

***

#### Does `imessage-exporter` export message conversations that are on a user's iPhone/iPad but not on the user's Mac?

No, `imessage-exporter` only reads data present on the host system.

***

#### How does the exporter handle previously exported messages?

All messages are exported every time `imessage-exporter` runs. `imessage-exporter` appends to files when writing, so make sure to specify a different location!

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

Yes, this tool cannot recover deleted messages.

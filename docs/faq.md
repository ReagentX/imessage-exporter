# Frequently Asked Questions

Q: I cannot connect to the messages database

A: Ensure your terminal emulator has [full disk access](https://kb.synology.com/en-us/C2/tutorial/How_to_enable_Full_Disk_Access_on_a_Mac) if using the default location or ensure that the path to the database file is correct.

***

Q: Does `imessage-exporter` export message conversations that are on a user's iPhone/iPad but not on the user's Mac?

A: No, `imessage-exporter` only reads data present on the host system.

***

Q: How does the exporter handle previously exported messages?

A: All messages are exported every time `imessage-exporter` runs. `imessage-exporter` appends to files when writing, so make sure to specify a different location!

***

Q: Is it possible to export a conversation and re-integrate it back onto another Apple ID?

A: No, I do not want to be trusted with write access to your iMessage data. This software is *read only*.

***

Q: Is there a search function?

A: No, this software just builds exports. I use [`ripgrep`](https://github.com/BurntSushi/ripgrep) to search though the exported files.

***

Q: Will it run on Windows?

A: I don't pre-build binaries for Windows, but it should compile to that target. As long as you can point it at an iMessage database, it should work.

***

Q: Are voice messages be saved?

A: Expired ones cannot because they are deleted. If you kept them then they are included in the exports.

***

Q: Are deleted messages simply erased from the database?
A: Yes, this tool cannot recover deleted messages.

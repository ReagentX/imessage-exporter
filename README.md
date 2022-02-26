# imessage-export

This crate provides both a library to interact with iMessage data as well as a binary that can perform some read-only operations using that data.

## Library

The `imessage_database` library provides models that allow us to access iMessage information as native data structures.

## Runtime

The `imessage_exporter` binary provides the ability to export iMessage data to `txt`, `csv` and rich `pdf` formats. It also has some diagnostic tooling to find problems with the iMessage database.

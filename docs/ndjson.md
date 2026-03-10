NDJSON Export
=============

> NDJSON: Newline-Delimited JSON

This document describes the NDJSON export produced by the
`imessage-exporter` binary and provides the schema, plus example
consumer code (Rust, Python, and Go).

## Overview

- Format: One JSON object per line, each line terminated by a linefeed
  character (decimal 10, hex 0A), as per the [NDJSON spec][].
- File extension: `.ndjson` (CLI accepts `--format ndjson`, `json`, or
  `jsonl`).
- Each record is self-contained (message metadata, sender, recipients,
  chat id, and attachment metadata), such that downstream tools should
  not need to consult the original database to interpret a message
  entry.

[NDJSON spec]: https://github.com/ndjson/ndjson-spec

## Message Schema

### MessageDto

Each NDJSON record is a JSON object with the following fields:

- `rowid`: integer — database rowid for the message
- `guid`: string — message GUID
- `date_iso`: string — ISO8601 local timestamp
  (e.g. "2022-05-17T20:29:42+02:00")
- `is_from_me`: boolean — whether the message is from the database
  owner
- `sender`: object — ParticipantDto describing the sender
- `recipients`: array of `ParticipantDto` — participants in the
  conversation (includes sender for group chats)
- `text`: string or null — message textual content (may include newlines; JSON string escaping is used)
- `chat_id`: integer or null — chat rowid when available
- `attachments`: array of `AttachmentDto` — zero or more attachments
  associated with the message

### ParticipantDto

- `handle_id`: integer or null — internal handle rowid when known
- `handle`: string or null — handle identifier (phone number or email)
  when available
- `display_name`: string or null — resolved display name when
  available (contact lookup result)
- `is_me`: boolean — whether this participant entry represents "Me",
  the owner of the message database the export is being generated from

### AttachmentDto

- `filename`: string or null — original filename when available
- `mime_type`: string or null — mime type when available
- `size`: integer — total bytes (may be 0 when unknown)
- `path`: string — resolved file path in the export (relative or
  absolute depending on export options)

### Escaping Rules

- NDJSON relies on each line being a complete JSON object. Message
  text may contain newlines; these are included inside the JSON string
  using standard JSON escaping and do not break record boundaries.
- Consumers should parse each line as JSON rather than attempting to
  split on raw newlines inside strings.

## Parsing

These examples are meant to be explanatory, and are not guaranteed to
work in any particular use case; design and test all implementation
code appropriately.

### Rust 

Using `serde_json` and streaming parser:

```rust
use std::fs::File;
use std::io::{BufRead, BufReader};
use serde::Deserialize;

#[derive(Deserialize, Debug)]
struct ParticipantDto { /* fields matching schema */ }

#[derive(Deserialize, Debug)]
struct AttachmentDto { /* fields matching schema */ }

#[derive(Deserialize, Debug)]
struct MessageDto {
    rowid: i32,
    guid: String,
    date_iso: String,
    is_from_me: bool,
    sender: ParticipantDto,
    recipients: Vec<ParticipantDto>,
    text: Option<String>,
    chat_id: Option<i32>,
    attachments: Vec<AttachmentDto>,
}

fn stream_ndjson(path: &str) -> Result<(), Box<dyn std::error::Error>> {
    let f = File::open(path)?;
    let reader = BufReader::new(f);
    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() { continue; }
        let msg: MessageDto = serde_json::from_str(&line)?;
        println!("Got message {}", msg.rowid);
    }
    Ok(())
}
```

### Python 

Using `json` or `ijson` for streaming:

```python
import json

def stream_ndjson(path):
    with open(path, 'r', encoding='utf-8') as f:
        for line in f:
            line = line.strip()
            if not line:
                continue
            msg = json.loads(line)
            print('rowid', msg.get('rowid'))
```

### Go 

Using `bufio` + encoding/json:

```go
package main

import (
    "bufio"
    "encoding/json"
    "fmt"
    "os"
)

type ParticipantDto struct {
    HandleID    *int   `json:"handle_id"`
    Handle      *string `json:"handle"`
    DisplayName *string `json:"display_name"`
    IsMe        bool   `json:"is_me"`
}

type AttachmentDto struct {
    Filename *string `json:"filename"`
    MimeType *string `json:"mime_type"`
    Size     int64   `json:"size"`
    Path     string  `json:"path"`
}

type MessageDto struct {
    Rowid      int64             `json:"rowid"`
    Guid       string            `json:"guid"`
    DateISO    string            `json:"date_iso"`
    IsFromMe   bool              `json:"is_from_me"`
    Sender     ParticipantDto    `json:"sender"`
    Recipients []ParticipantDto  `json:"recipients"`
    Text       *string           `json:"text"`
    ChatID     *int64            `json:"chat_id"`
    Attachments []AttachmentDto  `json:"attachments"`
}

func streamNdjson(path string) error {
    f, err := os.Open(path)
    if err != nil { return err }
    defer f.Close()
    scanner := bufio.NewScanner(f)
    for scanner.Scan() {
        line := scanner.Text()
        if len(line) == 0 { continue }
        var m MessageDto
        if err := json.Unmarshal([]byte(line), &m); err != nil {
            return err
        }
        fmt.Println("rowid", m.Rowid)
    }
    return scanner.Err()
}
```

## Best Practices

Some tips for designing consumers of this data:

- Treat each line as an independent JSON object and use a real JSON
  parser to decode it.
- Use streaming parsers to control memory usage, or investigate
  message sizes before loading; don't not make assumptions about the
  maximum message size, since the theoretical upper bound is high.
- Validate invariants when useful:
  - `sender.is_me == is_from_me` (the exporter maintains this invariant)
  - Recipients _should_ include the sender for group chats
- Do not attempt to split JSON records by searching raw bytes for
  `}\n{`; instead rely on line delimiting and JSON parsing.

### Future Evolution

- The NDJSON schema may be extended with additional fields in future
  releases. Consumers should ignore unknown keys when parsing (most
  JSON libraries do this by default, at least when using permissive
  unmarshalling into `map[string]interface{}` or structs with
  `omitempty`/pointer types).
- Embedded attachments _might_ be included in future releases, likely
  with Base64 encoding of the raw bytes to make the output more
  manageable.

## Sample Data

```json
{"rowid":1,"guid":"ABC-...","date_iso":"2022-05-17T20:29:42+02:00","is_from_me":true,"sender":{"handle_id":null,"handle":"+1555555","display_name":"Me","is_me":true},"recipients":[{"handle_id":2,"handle":"+1444444","display_name":"Alice","is_me":false}],"text":"Hello Alice!\nHow are you?","chat_id":42,"attachments":[]}
```

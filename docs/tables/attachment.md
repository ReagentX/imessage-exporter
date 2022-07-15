# Attachment Table Structure

| Column | Type | Effect |
| -- | -- | -- |
| ROWID |  INTEGER | PRIMARY KEY AUTOINCREMENT |
| guid |  TEXT | UNIQUE NOT NULL |
| created_date |  INTEGER | DEFAULT 0 |
| start_date |  INTEGER | DEFAULT 0 |
| filename |  TEXT | |
| uti |  TEXT | |
| mime_type |  TEXT | |
| transfer_state |  INTEGER | DEFAULT 0 |
| is_outgoing |  INTEGER | DEFAULT 0 |
| user_info |  BLOB | |
| transfer_name |  TEXT | |
| total_bytes |  INTEGER | DEFAULT 0 |
| is_sticker |  INTEGER | DEFAULT 0 |
| sticker_user_info |  BLOB | |
| attribution_info |  BLOB | |
| hide_attachment |  INTEGER | DEFAULT 0 |
| ck_sync_state |  INTEGER | DEFAULT 0 |
| ck_server_change_token_blob |  BLOB | DEFAULT NULL |
| ck_record_id |  TEXT | DEFAULT NULL |
| original_guid |  TEXT | |
| sr_ck_record_id |  TEXT | DEFAULT NULL |
| sr_ck_sync_state |  INTEGER | DEFAULT 0 |
| sr_ck_server_change_token_blob |  BLOB | DEFAULT NULL |
| is_commsafety_sensitive |  INTEGER | DEFAULT 0 |

# Chat Table Structure

| Column | Type | Effect |
| -- | -- | -- |
| ROWID | INTEGER | PRIMARY KEY AUTOINCREMENT |
| guid | TEXT | UNIQUE NOT NULL |
| style | INTEGER | |
| state | INTEGER | |
| account_id | TEXT | |
| properties | BLOB | |
| chat_identifier | TEXT | |
| service_name | TEXT | |
| room_name | TEXT | |
| account_login | TEXT | |
| is_archived | INTEGER | DEFAULT 0 |
| last_addressed_handle | TEXT | |
| display_name | TEXT | |
| group_id | TEXT | |
| is_filtered | INTEGER | DEFAULT 0 |
| successful_query | INTEGER | DEFAULT 1 |
| engram_id | TEXT | |
| server_change_token | TEXT | |
| ck_sync_state | INTEGER | DEFAULT 0 |
| last_read_message_timestamp | INTEGER | DEFAULT 0 |
| ck_record_system_property_blob | BLOB | |
| original_group_id | TEXT | DEFAULT NULL |
| sr_server_change_token | TEXT | |
| sr_ck_sync_state | INTEGER | DEFAULT 0 |
| cloudkit_record_id | TEXT | DEFAULT NULL |
| sr_cloudkit_record_id | TEXT | DEFAULT NULL |
| last_addressed_sim_id | TEXT | DEFAULT NULL |
| is_blackholed | INTEGER | DEFAULT 0 |
| syndication_date | INTEGER | DEFAULT 0 |
| syndication_type | INTEGER | DEFAULT 0 |

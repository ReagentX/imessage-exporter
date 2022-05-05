# Messages Table Structure

| Column | Type | Effect |
| -- | -- | -- |
| ROWID | INTEGER |  PRIMARY KEY AUTOINCREMENT |
| guid | TEXT |  UNIQUE NOT NULL |
| text | TEXT |  |
| replace | INTEGER |  DEFAULT 0 |
| service_center | TEXT |  |
| handle_id | INTEGER |  DEFAULT 0 |
| subject | TEXT |  |
| country | TEXT |  |
| attributedBody | BLOB |  |
| version | INTEGER |  DEFAULT 0 |
| type | INTEGER |  DEFAULT 0 |
| service | TEXT |  |
| account | TEXT |  |
| account_guid | TEXT |  |
| error | INTEGER |  DEFAULT 0 |
| date | INTEGER |  |
| date_read | INTEGER |  |
| date_delivered | INTEGER |  |
| is_delivered | INTEGER |  DEFAULT 0 |
| is_finished | INTEGER |  DEFAULT 0 |
| is_emote | INTEGER |  DEFAULT 0 |
| is_from_me | INTEGER |  DEFAULT 0 |
| is_empty | INTEGER |  DEFAULT 0 |
| is_delayed | INTEGER |  DEFAULT 0 |
| is_auto_reply | INTEGER |  DEFAULT 0 |
| is_prepared | INTEGER |  DEFAULT 0 |
| is_read | INTEGER |  DEFAULT 0 |
| is_system_message | INTEGER |  DEFAULT 0 |
| is_sent | INTEGER |  DEFAULT 0 |
| has_dd_results | INTEGER |  DEFAULT 0 |
| is_service_message | INTEGER |  DEFAULT 0 |
| is_forward | INTEGER |  DEFAULT 0 |
| was_downgraded | INTEGER |  DEFAULT 0 |
| is_archive | INTEGER |  DEFAULT 0 |
| cache_has_attachments | INTEGER |  DEFAULT 0 |
| cache_roomnames | TEXT |  |
| was_data_detected | INTEGER |  DEFAULT 0 |
| was_deduplicated | INTEGER |  DEFAULT 0 |
| is_audio_message | INTEGER |  DEFAULT 0 |
| is_played | INTEGER |  DEFAULT 0 |
| date_played | INTEGER |  |
| item_type | INTEGER |  DEFAULT 0 |
| other_handle | INTEGER |  DEFAULT 0 |
| group_title | TEXT |  |
| group_action_type | INTEGER |  DEFAULT 0 |
| share_status | INTEGER |  DEFAULT 0 |
| share_direction | INTEGER |  DEFAULT 0 |
| is_expirable | INTEGER |  DEFAULT 0 |
| expire_state | INTEGER |  DEFAULT 0 |
| message_action_type | INTEGER |  DEFAULT 0 |
| message_source | INTEGER |  DEFAULT 0 |
| associated_message_guid | STRING |  DEFAULT NULL |
| balloon_bundle_id | STRING |  DEFAULT NULL |
| payload_data | BLOB |  |
| associated_message_type | INTEGER |  DEFAULT 0 |
| expressive_send_style_id | STRING |  DEFAULT NULL |
| associated_message_range_location | INTEGER |  DEFAULT 0 |
| associated_message_range_length | INTEGER |  DEFAULT 0 |
| time_expressive_send_played | INTEGER |  DEFAULT 0 |
| message_summary_info | BLOB |  DEFAULT NULL |
| ck_sync_state | INTEGER |  DEFAULT 0 |
| ck_record_id | TEXT |  DEFAULT NULL |
| ck_record_change_tag | TEXT |  DEFAULT NULL |
| destination_caller_id | TEXT |  DEFAULT NULL |
| sr_ck_sync_state | INTEGER |  DEFAULT 0 |
| sr_ck_record_id | TEXT |  DEFAULT NULL |
| sr_ck_record_change_tag | TEXT |  DEFAULT NULL |
| is_corrupt | INTEGER |  DEFAULT 0 |
| reply_to_guid | TEXT |  DEFAULT NULL |
| sort_id | INTEGER |  DEFAULT 0 |
| is_spam | INTEGER |  DEFAULT 0 |
| has_unseen_mention | INTEGER |  DEFAULT 0 |
| thread_originator_guid | TEXT |  DEFAULT NULL |
| thread_originator_part | TEXT |  DEFAULT NULL |
| syndication_ranges | TEXT |  DEFAULT NULL |
| was_delivered_quietly | INTEGER |  DEFAULT 0 |
| did_notify_recipient | INTEGER |  DEFAULT 0 |
| synced_syndication_ranges | TEXT |  DEFAULT NULL |

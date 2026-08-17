/*!
 Application runtime state and export lifecycle.
*/

use std::{
    cmp::min,
    collections::{BTreeSet, HashMap, HashSet},
    fs::create_dir_all,
    path::{Path, PathBuf},
};

use fdlimit::raise_fd_limit;
use fs2::available_space;
use imessage_database::{
    message_types::translation::Translation,
    tables::{
        attachment::Attachment,
        chat::Chat,
        chat_handle::ChatToHandle,
        handle::Handle,
        messages::Message,
        table::{ATTACHMENTS_DIR, Cacheable, ME, ORPHANED, UNKNOWN, get_db_size},
    },
    util::{
        dates::{format as format_date, get_local_time, get_offset, readable_diff},
        size::format_file_size,
    },
};

use crate::{
    HTML, TXT,
    app::{
        compatibility::attachment_manager::AttachmentManagerMode, contacts::Name,
        data_source::DataSource, error::RuntimeError, export_type::ExportType, options::Options,
        sanitizers::sanitize_filename,
    },
    exporters::shared::driver::run_export,
};

// Maximum filename length before accounting for the export path.
const MAX_LENGTH: usize = 235;

// MARK: Config
/// Cached application state used during export.
pub struct Config {
    /// Chat rows keyed by chat row ID.
    pub chatrooms: HashMap<i32, Chat>,
    /// Deduplicated chat IDs keyed by chat row ID.
    pub real_chatrooms: HashMap<i32, i32>,
    /// Participant handle IDs keyed by chat row ID.
    pub chatroom_participants: HashMap<i32, BTreeSet<i32>>,
    /// Contact names keyed by deduplicated participant ID.
    pub participants: HashMap<i32, Name>,
    /// Deduplicated participant IDs keyed by handle row ID.
    pub real_participants: HashMap<i32, i32>,
    /// Tapback messages keyed by target message GUID and body component index.
    pub tapbacks: HashMap<String, HashMap<usize, Vec<Message>>>,
    /// Message GUIDs with translation metadata.
    pub translated_messages: HashSet<String>,
    /// Parsed application options.
    pub options: Options,
    /// Unix timestamp offset for the Messages epoch.
    pub offset: i64,
    /// Database and contact data source.
    pub data_source: DataSource,
}

impl Config {
    /// Return the chat and deduplicated chat ID for a message.
    pub fn conversation(&self, message: &Message) -> Option<(&Chat, &i32)> {
        match message.chat_id.or(message.deleted_from) {
            Some(chat_id) => {
                if let Some(chatroom) = self.chatrooms.get(&chat_id) {
                    self.real_chatrooms.get(&chat_id).map(|id| (chatroom, id))
                } else {
                    eprintln!("Chat ID {chat_id} does not exist in chat table!");
                    None
                }
            }
            // No chat_id provided
            None => None,
        }
    }

    /// Whether `message` has a cached translation (and is therefore worth
    /// querying the database for it).
    pub fn is_translated(&self, message: &Message) -> bool {
        self.translated_messages.contains(&message.guid)
    }

    /// The cached [`Translation`] for `message`, gated on [`Self::is_translated`]
    /// so untranslated messages never touch the database. A present-but-
    /// unparseable payload surfaces as a [`RuntimeError`] rather than being
    /// silently dropped, leaving the handling to the caller. The library's
    /// error is translated to [`RuntimeError`] at this boundary.
    pub fn translation_for(&self, message: &Message) -> Result<Option<Translation>, RuntimeError> {
        if self.is_translated(message) {
            Ok(message.get_translation(self.data_source.db())?)
        } else {
            Ok(None)
        }
    }

    /// Return the export attachment directory.
    pub fn attachment_path(&self) -> PathBuf {
        let mut path = self.options.export_path.clone();
        path.push(ATTACHMENTS_DIR);
        path
    }

    /// Return the per-conversation attachment directory name.
    pub fn conversation_attachment_path(&self, chat_id: Option<i32>) -> String {
        if let Some(chat_id) = chat_id
            && let Some(real_id) = self.real_chatrooms.get(&chat_id)
        {
            return real_id.to_string();
        }
        String::from(ORPHANED)
    }

    /// Return the export-relative path for an attachment.
    ///
    /// Copied attachments resolve relative to the export root; uncopied
    /// attachments resolve to their source path or stored filename.
    pub fn message_attachment_path(&self, attachment: &Attachment) -> String {
        match &attachment.copied_path {
            Some(path) => {
                if let Ok(relative_path) = path.strip_prefix(&self.options.export_path) {
                    return relative_path.display().to_string();
                }
                path.display().to_string()
            }
            None => attachment
                .resolved_attachment_path(
                    &self.options.platform,
                    &self.options.db_path,
                    self.options.attachment_root.as_deref(),
                )
                .unwrap_or_else(|| {
                    attachment
                        .filename()
                        .unwrap_or("Attachment missing name metadata!")
                        .to_string()
                }),
        }
    }

    /// Return an export-relative path when possible.
    pub fn relative_path(&self, path: &Path) -> String {
        if let Ok(relative_path) = path.strip_prefix(&self.options.export_path) {
            return relative_path.display().to_string();
        }
        path.display().to_string()
    }

    // MARK: Filenames
    /// Build an output filename for a chat.
    ///
    /// If the chat has an assigned name, use that, truncating if necessary.
    ///
    /// If it does not, use participant names. Failing that, use the chat
    /// identifier.
    pub fn filename(&self, chatroom: &Chat) -> String {
        // Account for the export path so the full output path stays under the limit.
        let export_path_len = self.options.export_path.as_os_str().len();
        let max_len = MAX_LENGTH.saturating_sub(export_path_len + 1);

        let mut filename = match &chatroom.display_name() {
            // If there is a display name, use that
            Some(name) => {
                let truncated_len = name.floor_char_boundary(min(max_len, name.len()));
                format!(
                    "{} - {}",
                    &name[..truncated_len],
                    // Get the deduplicated chat ID to ensure the filename is unique, even if the group name is not
                    self.real_chatrooms
                        .get(&chatroom.rowid)
                        .unwrap_or(&chatroom.rowid)
                )
            }
            // Fallback if there is no name set
            None => {
                if let Some(participants) = self.chatroom_participants.get(&chatroom.rowid) {
                    self.filename_from_participants(participants)
                } else {
                    eprintln!(
                        "Found error: message chat ID {} has no members!",
                        chatroom.rowid
                    );
                    chatroom.chat_identifier.clone()
                }
            }
        };

        // Add the extension to the filename
        if let Some(export_type) = &self.options.export_type {
            filename.push_str(export_type.extension());
        }

        sanitize_filename(&filename)
    }

    /// Build a filename from participant names, truncating when needed.
    ///
    /// - All names:
    ///   - Contact 1, Contact 2
    /// - Truncated Names
    ///   - Contact 1, Contact 2, ... Contact 13 and 4 others
    fn filename_from_participants(&self, participants: &BTreeSet<i32>) -> String {
        // Account for the export path so the full output path stays under the limit.
        let export_path_len = self.options.export_path.as_os_str().len();
        let max_len = MAX_LENGTH.saturating_sub(export_path_len + 1);

        let mut added = 0;
        let mut out_s = String::with_capacity(max_len);
        for participant_id in participants {
            let participant_details = match self.resolve_participant(*participant_id) {
                Some(name) => name.details.as_str(),
                None => UNKNOWN,
            };

            let separator = if out_s.is_empty() { "" } else { ", " };
            if participant_details.len() + separator.len() + out_s.len() <= max_len {
                out_s.push_str(separator);
                out_s.push_str(participant_details);
                added += 1;
            } else {
                let extra = format!(", and {} others", participants.len() - added);
                let space_remaining = extra.len() + out_s.len();
                if space_remaining >= max_len {
                    let start = out_s.floor_char_boundary(max_len.saturating_sub(extra.len()));
                    out_s.replace_range(start.., &extra);
                } else if out_s.is_empty() {
                    let truncated_len = participant_details
                        .floor_char_boundary(min(max_len, participant_details.len()));
                    out_s.push_str(&participant_details[..truncated_len]);
                } else {
                    out_s.push_str(&extra);
                }
                break;
            }
        }
        out_s
    }

    // MARK: Init
    /// Build application state and caches.
    pub fn new(options: Options) -> Result<Config, RuntimeError> {
        let data_source = DataSource::from(&options)?;

        eprintln!("Building cache...");
        eprintln!("  [1/5] Caching chats...");
        let chatrooms = Chat::cache(data_source.db())?;

        eprintln!("  [2/5] Caching chatrooms...");
        let chatroom_participants = ChatToHandle::cache(data_source.db())?;
        let chat_handle_lookup = ChatToHandle::get_chat_lookup_map(data_source.db())?;
        let real_chatrooms = ChatToHandle::dedupe(&chatroom_participants, &chat_handle_lookup)?;

        eprintln!("  [3/5] Caching participants...");
        let participants = Handle::cache(data_source.db())?;
        let real_participants = Handle::dedupe(&participants);
        let participants_map = data_source
            .contacts_index
            .build_participants_map(&participants, &real_participants);

        eprintln!("  [4/5] Caching tapbacks...");
        let tapbacks = Message::cache(data_source.db())?;

        eprintln!("  [5/5] Caching translations...");
        // Missing translation metadata means no messages need translation lookup.
        let translated_messages = Message::cache_translations(data_source.db()).unwrap_or_default();
        eprintln!("Cache built!");

        Ok(Config {
            chatrooms,
            real_chatrooms,
            chatroom_participants,
            real_participants,
            participants: participants_map,
            tapbacks,
            translated_messages,
            options,
            offset: get_offset(),
            data_source,
        })
    }

    // MARK: Filters
    /// Resolve a comma-separated participant filter into chat and handle IDs.
    pub(crate) fn resolve_filtered_handles(&mut self) {
        if let Some(conversation_filter) = &self.options.conversation_filter {
            let parsed_handle_filter = conversation_filter.split(',').collect::<Vec<&str>>();

            let mut included_chatrooms: BTreeSet<i32> = BTreeSet::new();
            let mut included_handles: BTreeSet<i32> = BTreeSet::new();

            // First, resolve matching participant names to handle IDs.
            self.participants.iter().for_each(|(_, handle_name)| {
                for included_name in &parsed_handle_filter {
                    if handle_name.contains(included_name) {
                        included_handles.extend(&handle_name.handle_ids);
                    }
                }
            });

            // Then include chats that contain any selected handle.
            self.chatroom_participants
                .iter()
                .for_each(|(chat_id, participants)| {
                    if !participants.is_disjoint(&included_handles) {
                        included_chatrooms.insert(*chat_id);
                    }
                });

            self.options
                .query_context
                .set_selected_handle_ids(included_handles);

            self.options
                .query_context
                .set_selected_chat_ids(included_chatrooms);

            self.log_filtered_handles_and_chats();
        }
    }

    /// Log the number of handles and chats selected by the participant filter.
    fn log_filtered_handles_and_chats(&self) {
        if let (Some(selected_handle_ids), Some(selected_chat_ids)) = (
            &self.options.query_context.selected_handle_ids,
            &self.options.query_context.selected_chat_ids,
        ) {
            let unique_handle_ids: HashSet<Option<&i32>> = selected_handle_ids
                .iter()
                .map(|handle_id| self.real_participants.get(handle_id))
                .collect();

            let mut unique_chat_ids: HashSet<String> = HashSet::new();
            for selected_chat_id in selected_chat_ids {
                if let Some(participants) = self.chatroom_participants.get(selected_chat_id) {
                    unique_chat_ids.insert(self.filename_from_participants(participants));
                }
            }

            eprintln!(
                "Filtering for {} handle{} across {} chatrooms...",
                unique_handle_ids.len(),
                if unique_handle_ids.len() == 1 {
                    ""
                } else {
                    "s"
                },
                unique_chat_ids.len()
            );
        }
    }

    /// Return the connected database file size.
    fn total_db_size(&self) -> Result<u64, RuntimeError> {
        let db_path = self
            .data_source
            .db()
            .path()
            .ok_or_else(|| RuntimeError::FileNameError {
                path: self.options.db_path.clone(),
                reason: "database connection has no associated path",
            })?;

        get_db_size(Path::new(db_path)).map_err(RuntimeError::from)
    }

    /// Validate available disk space for the requested export.
    fn ensure_free_space(&self) -> Result<(), RuntimeError> {
        // Exports are usually smaller than the database; use 10% as a
        // conservative estimate before adding attachment bytes.
        let total_db_size = self.total_db_size()?;
        let mut estimated_export_size = total_db_size / 10;

        let free_space_at_location = available_space(&self.options.export_path)?;

        if let AttachmentManagerMode::Disabled = self.options.attachment_manager.mode {
            if estimated_export_size >= free_space_at_location {
                return Err(RuntimeError::NotEnoughAvailableSpace(
                    estimated_export_size,
                    free_space_at_location,
                ));
            }
        } else {
            let total_attachment_size = Attachment::get_total_attachment_bytes(
                self.data_source.db(),
                &self.options.query_context,
            )?;
            estimated_export_size += total_attachment_size;
            if estimated_export_size >= free_space_at_location {
                return Err(RuntimeError::NotEnoughAvailableSpace(
                    estimated_export_size,
                    free_space_at_location,
                ));
            }
        }

        println!(
            "Estimated export size: {}",
            format_file_size(estimated_export_size)
        );

        Ok(())
    }

    // MARK: Diagnostic
    /// Print diagnostic data for the active database and environment.
    fn run_diagnostic(&self) -> Result<(), RuntimeError> {
        println!("\niMessage Database Diagnostics\n");

        // Handle diagnostics
        let handle_diag = Handle::run_diagnostic(self.data_source.db())?;
        println!("Handle diagnostic data:");
        println!("    Total handles: {}", handle_diag.total_handles);
        if let Some(handles_with_multiple_ids) = handle_diag.handles_with_multiple_ids
            && handles_with_multiple_ids > 0
        {
            println!(
                "    Handles with more than one ID: {}",
                handles_with_multiple_ids
            );
        }
        if handle_diag.total_duplicated > 0 {
            println!(
                "    Total duplicated handles: {}",
                handle_diag.total_duplicated
            );
        }

        // Message diagnostics
        let message_diag = Message::run_diagnostic(self.data_source.db())?;
        println!("Message diagnostic data:");
        println!("    Total messages: {}", message_diag.total_messages);
        if message_diag.messages_without_chat > 0 {
            println!(
                "    Messages not associated with a chat: {}",
                message_diag.messages_without_chat
            );
        }
        if message_diag.messages_in_multiple_chats > 0 {
            println!(
                "    Messages belonging to more than one chat: {}",
                message_diag.messages_in_multiple_chats
            );
        }
        if let Some(recoverable_messages) = message_diag.recoverable_messages
            && recoverable_messages > 0
        {
            println!("    Recoverable deleted messages: {}", recoverable_messages);
        }
        if let (Some(first), Some(last)) = (
            message_diag.first_message_date,
            message_diag.last_message_date,
        ) && let (Ok(first_date), Ok(last_date)) = (
            get_local_time(first, self.offset),
            get_local_time(last, self.offset),
        ) {
            println!(
                "    Date range: {} to {}\n                {}",
                format_date(&first_date),
                format_date(&last_date),
                readable_diff(&first_date, &last_date).unwrap_or_else(|| "N/A".to_string()),
            );
        }

        // Attachment diagnostics
        let attach_diag = Attachment::run_diagnostic(
            self.data_source.db(),
            &self.options.db_path,
            &self.options.platform,
            self.options.attachment_root.as_deref(),
        )?;
        if attach_diag.total_attachments > 0 {
            println!("Attachment diagnostic data:");
            println!("    Total attachments: {}", attach_diag.total_attachments);
            println!(
                "        Data referenced in table: {}",
                format_file_size(attach_diag.total_bytes_referenced)
            );
            println!(
                "        Data present on disk: {}",
                format_file_size(attach_diag.total_bytes_on_disk)
            );
            if attach_diag.missing_files > 0 {
                println!(
                    "    Missing files: {} ({:.0}%)",
                    attach_diag.missing_files,
                    attach_diag.missing_percent().unwrap_or(0.0)
                );
                println!("        No path provided: {}", attach_diag.no_path_provided);
                println!("        No file located: {}", attach_diag.no_file_located());
            }
        }

        // Chat/thread diagnostics
        let chat_diag = ChatToHandle::run_diagnostic(self.data_source.db())?;
        println!("Thread diagnostic data:");
        println!("    Total chats: {}", chat_diag.total_chats);
        if chat_diag.total_duplicated > 0 {
            println!("    Total duplicated chats: {}", chat_diag.total_duplicated);
        }
        if chat_diag.chats_with_no_handles > 0 {
            println!(
                "    Chats with no handles: {}",
                chat_diag.chats_with_no_handles
            );
        }

        // Global Diagnostics
        println!("Global diagnostic data:");

        let total_db_size = self.total_db_size()?;
        println!(
            "    Total database size: {}",
            format_file_size(total_db_size)
        );

        // For each handle in the participants list, count how many have matches in the contacts index
        let total_resolved =
            self.participants.iter().fold(
                0,
                |acc, (_, name)| {
                    if name.full.is_empty() { acc } else { acc + 1 }
                },
            );

        let resolved_percent = if self.participants.is_empty() {
            0.0
        } else {
            (total_resolved as f32 / self.participants.len() as f32 * 100.0).round()
        };
        println!(
            "    Handles with resolved names: {}/{} ({resolved_percent}%)",
            total_resolved,
            self.participants.len(),
        );

        println!("\nEnvironment Diagnostics\n");
        self.options.attachment_manager.diagnostic();

        Ok(())
    }

    // MARK: Startup
    /// Run diagnostics or export data, depending on the selected options.
    pub fn start(&self) -> Result<(), RuntimeError> {
        if self.options.diagnostic {
            self.run_diagnostic()?;
        } else if let Some(export_type) = &self.options.export_type {
            if let Some(filters) = &self.options.conversation_filter
                && !self.options.query_context.has_filters()
            {
                return Err(RuntimeError::InvalidOptions(format!(
                    "Selected filter `{filters}` does not match any participants!"
                )));
            }

            // Ensure the path we want to export to exists
            create_dir_all(&self.options.export_path)?;

            // Ensure the path we want to copy attachments to exists, if requested
            if !matches!(
                self.options.attachment_manager.mode,
                AttachmentManagerMode::Disabled
            ) {
                create_dir_all(self.attachment_path())?;
            }

            // Ensure there is enough free disk space to write the export
            if !self.options.ignore_disk_space {
                self.ensure_free_space()?;
            }

            // Ensure we have enough file handles to export
            let _ = raise_fd_limit();

            // Create exporter, pass it data we care about, then kick it off
            match export_type {
                ExportType::Html => {
                    run_export(&mut HTML::new(self)?)?;
                }
                ExportType::Txt => {
                    run_export(&mut TXT::new(self)?)?;
                }
            }
        }
        println!("Done!");
        Ok(())
    }

    /// Resolve the display name for a message sender.
    pub fn who<'a, 'b: 'a>(
        &'a self,
        handle_id: Option<i32>,
        is_from_me: bool,
        destination_caller_id: &'b Option<String>,
    ) -> &'a str {
        if is_from_me {
            if self.options.use_caller_id {
                return destination_caller_id.as_deref().unwrap_or(ME);
            }
            return self.options.custom_name.as_deref().unwrap_or(ME);
        } else if let Some(handle_id) = handle_id {
            return match self.resolve_participant(handle_id) {
                Some(contact) => contact.get_display_name(),
                None => UNKNOWN,
            };
        }
        UNKNOWN
    }

    /// Resolve a participant name from a handle ID.
    fn resolve_participant(&self, handle_id: i32) -> Option<&Name> {
        if let Some(internal_id) = self.real_participants.get(&handle_id) {
            return self.participants.get(internal_id);
        }
        None
    }
}

// MARK: Test Config
#[cfg(test)]
impl Config {
    pub fn fake_app(options: Options) -> Config {
        let data_source = DataSource::from(&options).unwrap();

        Config {
            chatrooms: HashMap::new(),
            real_chatrooms: HashMap::new(),
            chatroom_participants: HashMap::new(),
            participants: HashMap::new(),
            real_participants: HashMap::new(),
            tapbacks: HashMap::new(),
            translated_messages: HashSet::new(),
            options,
            offset: get_offset(),
            data_source,
        }
    }

    pub fn fake_message() -> Message {
        Message {
            rowid: i32::default(),
            guid: String::default(),
            text: None,
            service: Some("iMessage".to_string()),
            handle_id: Some(i32::default()),
            destination_caller_id: None,
            subject: None,
            date: i64::default(),
            date_read: i64::default(),
            date_delivered: i64::default(),
            is_from_me: false,
            is_read: false,
            item_type: 0,
            other_handle: None,
            share_status: false,
            share_direction: None,
            group_title: None,
            group_action_type: 0,
            associated_message_guid: None,
            associated_message_type: Some(i32::default()),
            balloon_bundle_id: None,
            expressive_send_style_id: None,
            thread_originator_guid: None,
            thread_originator_part: None,
            date_edited: 0,
            associated_message_emoji: None,
            chat_id: None,
            num_attachments: 0,
            deleted_from: None,
            num_replies: 0,
            filter_action: None,
            filter_sub_action: None,
            components: vec![],
            edited_parts: None,
        }
    }

    pub(crate) fn fake_attachment() -> Attachment {
        Attachment {
            rowid: 0,
            guid: None,
            filename: Some("a/b/c/d.jpg".to_string()),
            uti: Some("public.png".to_string()),
            mime_type: Some("image/png".to_string()),
            transfer_name: Some("d.jpg".to_string()),
            total_bytes: 100,
            is_sticker: false,
            hide_attachment: 0,
            emoji_description: None,
            copied_path: None,
        }
    }
}

// MARK: Tests
#[cfg(test)]
mod filename_tests {
    use crate::{
        Config, Options,
        app::{contacts::Name, runtime::MAX_LENGTH},
    };

    use imessage_database::tables::chat::Chat;

    use std::{collections::BTreeSet, path::PathBuf};

    pub fn fake_chat() -> Chat {
        Chat {
            rowid: 0,
            chat_identifier: "Default".to_string(),
            service_name: Some(String::new()),
            display_name: None,
        }
    }

    #[test]
    fn can_create() {
        let mut options = Options::fake_options(crate::app::export_type::ExportType::Html);
        // Disable the export
        options.export_type = None;
        let app = Config::fake_app(options);
        app.start().unwrap();
    }

    #[test]
    fn can_get_filename_good() {
        let options = Options::fake_options(crate::app::export_type::ExportType::Html);
        let mut app = Config::fake_app(options);

        app.participants.insert(10, Name::fake_name("Person 10"));
        app.participants.insert(11, Name::fake_name("Person 11"));
        app.real_participants.insert(10, 10);
        app.real_participants.insert(11, 11);

        // Add participants
        let mut people = BTreeSet::new();
        people.insert(10);
        people.insert(11);

        let filename = app.filename_from_participants(&people);
        assert_eq!(filename, "Person 10, Person 11".to_string());
        assert!(filename.len() <= MAX_LENGTH);
    }

    #[test]
    fn can_get_filename_long_multiple() {
        let options = Options::fake_options(crate::app::export_type::ExportType::Html);
        let mut app = Config::fake_app(options);
        app.options.export_path = PathBuf::from("/tmp");

        app.participants.insert(
            10,
            Name::fake_name("Person With An Extremely and Excessively Long Name 10"),
        );
        app.participants.insert(
            11,
            Name::fake_name("Person With An Extremely and Excessively Long Name 11"),
        );
        app.participants.insert(
            12,
            Name::fake_name("Person With An Extremely and Excessively Long Name 12"),
        );
        app.participants.insert(
            13,
            Name::fake_name("Person With An Extremely and Excessively Long Name 13"),
        );
        app.participants.insert(
            14,
            Name::fake_name("Person With An Extremely and Excessively Long Name 14"),
        );
        app.participants.insert(
            15,
            Name::fake_name("Person With An Extremely and Excessively Long Name 15"),
        );
        app.participants.insert(
            16,
            Name::fake_name("Person With An Extremely and Excessively Long Name 16"),
        );
        app.participants.insert(
            17,
            Name::fake_name("Person With An Extremely and Excessively Long Name 17"),
        );
        app.real_participants.insert(10, 10);
        app.real_participants.insert(11, 11);
        app.real_participants.insert(12, 12);
        app.real_participants.insert(13, 13);
        app.real_participants.insert(14, 14);
        app.real_participants.insert(15, 15);
        app.real_participants.insert(16, 16);
        app.real_participants.insert(17, 17);

        // Add participants
        let mut people = BTreeSet::new();
        people.insert(10);
        people.insert(11);
        people.insert(12);
        people.insert(13);
        people.insert(14);
        people.insert(15);
        people.insert(16);
        people.insert(17);

        let filename = app.filename_from_participants(&people);
        assert_eq!(filename, "Person With An Extremely and Excessively Long Name 10, Person With An Extremely and Excessively Long Name 11, Person With An Extremely and Excessively Long Name 12, Person With An Extremely and Excessively Long Name , and 4 others".to_string());
        assert!(filename.len() <= MAX_LENGTH);
    }

    #[test]
    fn can_get_filename_single_long() {
        let options = Options::fake_options(crate::app::export_type::ExportType::Html);
        let mut app = Config::fake_app(options);
        app.options.export_path = PathBuf::from("/tmp");

        app.participants.insert(10, Name::fake_name("He slipped his key into the lock, and we all very quietly entered the cell. The sleeper half turned, and then settled down once more into a deep slumber. Holmes stooped to the water-jug, moistened his sponge, and then rubbed it twice vigorously across and down the prisoner's face."));
        app.real_participants.insert(10, 10);

        // Add 1 person
        let mut people = BTreeSet::new();
        people.insert(10);

        let filename = app.filename_from_participants(&people);
        assert_eq!(filename, "He slipped his key into the lock, and we all very quietly entered the cell. The sleeper half turned, and then settled down once more into a deep slumber. Holmes stooped to the water-jug, moistened his sponge, and then rubbed it tw".to_string());
        assert!(filename.len() <= MAX_LENGTH);
    }

    #[test]
    fn can_get_filename_respects_separator_length() {
        let options = Options::fake_options(crate::app::export_type::ExportType::Html);
        let mut app = Config::fake_app(options);

        let export_path_len = app.options.export_path.as_os_str().len();
        let max_len = MAX_LENGTH.saturating_sub(export_path_len + 1);

        // P + N = max_len - 1 passes the raw-bytes check; pushing ", " would overflow.
        let first_len = max_len / 2;
        let second_len = max_len - first_len - 1;
        let first = "a".repeat(first_len);
        let second = "b".repeat(second_len);

        app.participants.insert(10, Name::fake_name(&first));
        app.participants.insert(11, Name::fake_name(&second));
        app.real_participants.insert(10, 10);
        app.real_participants.insert(11, 11);

        let mut people = BTreeSet::new();
        people.insert(10);
        people.insert(11);

        let actual = app.filename_from_participants(&people);
        assert!(actual.len() <= max_len);
    }

    #[test]
    fn can_get_filename_chat_display_name_long() {
        let options = Options::fake_options(crate::app::export_type::ExportType::Html);
        let mut app = Config::fake_app(options);
        app.options.export_path = PathBuf::from("/tmp");

        let mut chat = fake_chat();
        chat.display_name = Some("Life is infinitely stranger than anything which the mind of man could invent. We would not dare to conceive the things which are really mere commonplaces of existence. If we could fly out of that window hand in hand, hover over this great city, gently remove the roofs".to_string());

        let filename = app.filename(&chat);
        assert_eq!(
            filename,
            "Life is infinitely stranger than anything which the mind of man could invent. We would not dare to conceive the things which are really mere commonplaces of existence. If we could fly out of that window hand in hand, hover over th - 0.html"
        );
    }

    #[test]
    fn can_get_filename_chat_display_name_normal() {
        let options = Options::fake_options(crate::app::export_type::ExportType::Html);
        let app = Config::fake_app(options);

        let mut chat = fake_chat();
        chat.display_name = Some("Test Chat Name".to_string());

        let filename = app.filename(&chat);
        assert_eq!(filename, "Test Chat Name - 0.html");
    }

    #[test]
    fn can_get_filename_chat_display_name_short() {
        let options = Options::fake_options(crate::app::export_type::ExportType::Html);
        let app = Config::fake_app(options);

        let mut chat = fake_chat();
        chat.display_name = Some("🤠".to_string());

        let filename = app.filename(&chat);
        assert_eq!(filename, "🤠 - 0.html");
    }

    #[test]
    fn can_get_filename_chat_participants() {
        let options = Options::fake_options(crate::app::export_type::ExportType::Html);
        let mut app = Config::fake_app(options);

        let chat = fake_chat();

        app.participants.insert(10, Name::fake_name("Person 10"));
        app.participants.insert(11, Name::fake_name("Person 11"));
        app.real_participants.insert(10, 10);
        app.real_participants.insert(11, 11);

        // Add participants
        let mut people = BTreeSet::new();
        people.insert(10);
        people.insert(11);
        app.chatroom_participants.insert(chat.rowid, people);

        let filename = app.filename(&chat);
        assert_eq!(filename, "Person 10, Person 11.html");
    }

    #[test]
    fn can_get_filename_chat_no_participants() {
        let options = Options::fake_options(crate::app::export_type::ExportType::Html);
        let app = Config::fake_app(options);

        let chat = fake_chat();

        let filename = app.filename(&chat);
        assert_eq!(filename, "Default.html");
    }

    #[test]
    fn can_get_filename_chat_display_name_truncated_emoji() {
        let options = Options::fake_options(crate::app::export_type::ExportType::Html);
        let app = Config::fake_app(options);

        // Boundary case: a display name exactly at the limit with a multi-byte emoji.
        // Each 🤠 is 4 bytes. Fill enough to force truncation at an emoji boundary.
        let emoji_name: String = "🤠".repeat(60); // 240 bytes, exceeds MAX_LENGTH
        let mut chat = fake_chat();
        chat.display_name = Some(emoji_name);

        // Should not panic, and the result should be valid UTF-8
        let filename = app.filename(&chat);
        assert!(filename.len() <= MAX_LENGTH + 20); // suffix " - 0.html" adds some
        // Verify it's valid UTF-8 (would fail to compile/run if not)
        assert!(filename.ends_with(".html"));
    }

    #[test]
    fn can_get_filename_single_long_emoji() {
        let options = Options::fake_options(crate::app::export_type::ExportType::Html);
        let mut app = Config::fake_app(options);

        // Boundary case: participant names made of 4-byte emoji.
        let emoji_name: String = "🌍".repeat(60); // 240 bytes
        app.participants.insert(10, Name::fake_name(&emoji_name));
        app.real_participants.insert(10, 10);

        let mut people = BTreeSet::new();
        people.insert(10);

        // Should not panic and should produce valid UTF-8
        let filename = app.filename_from_participants(&people);
        assert!(filename.len() <= MAX_LENGTH);
        // Verify the truncation happened on a char boundary
        for c in filename.chars() {
            assert!(c == '🌍');
        }
    }

    #[test]
    fn can_get_filename_multiple_long_emoji() {
        let options = Options::fake_options(crate::app::export_type::ExportType::Html);
        let mut app = Config::fake_app(options);

        // Boundary case: emoji names long enough to trigger "and N others".
        for i in 10..18 {
            let emoji_name: String = "🎵".repeat(30); // 120 bytes each
            app.participants.insert(i, Name::fake_name(&emoji_name));
            app.real_participants.insert(i, i);
        }

        let mut people = BTreeSet::new();
        for i in 10..18 {
            people.insert(i);
        }

        // Should not panic and should produce valid UTF-8 within the length limit
        let filename = app.filename_from_participants(&people);
        assert!(filename.len() <= MAX_LENGTH);
    }

    #[test]
    fn can_get_filename_cjk_truncation() {
        let options = Options::fake_options(crate::app::export_type::ExportType::Html);
        let mut app = Config::fake_app(options);

        // CJK characters are 3 bytes each; test truncation mid-character
        let cjk_name: String = "你".repeat(80); // 240 bytes
        app.participants.insert(10, Name::fake_name(&cjk_name));
        app.real_participants.insert(10, 10);

        let mut people = BTreeSet::new();
        people.insert(10);

        let filename = app.filename_from_participants(&people);
        assert!(filename.len() <= MAX_LENGTH);
        // All characters should be valid
        for c in filename.chars() {
            assert!(c == '你');
        }
    }
}

#[cfg(test)]
mod who_tests {
    use crate::{
        Config, Options,
        app::{contacts::Name, runtime::filename_tests::fake_chat},
    };

    #[test]
    fn can_get_who_them() {
        let options = Options::fake_options(crate::app::export_type::ExportType::Html);
        let mut app = Config::fake_app(options);

        app.participants.insert(10, Name::fake_name("Person 10"));
        app.real_participants.insert(10, 10);

        let who = app.who(Some(10), false, &None);
        assert_eq!(who, "Person 10".to_string());
    }

    #[test]
    fn can_get_who_them_missing() {
        let options = Options::fake_options(crate::app::export_type::ExportType::Html);
        let app = Config::fake_app(options);

        let who = app.who(Some(10), false, &None);
        assert_eq!(who, "Unknown".to_string());
    }

    #[test]
    fn can_get_who_me() {
        let options = Options::fake_options(crate::app::export_type::ExportType::Html);
        let app = Config::fake_app(options);

        let who = app.who(Some(0), true, &None);
        assert_eq!(who, "Me".to_string());
    }

    #[test]
    fn can_get_who_me_caller_id() {
        let mut options = Options::fake_options(crate::app::export_type::ExportType::Html);
        options.use_caller_id = true;
        let app = Config::fake_app(options);

        let caller_id = Some("test".to_string());
        let who = app.who(Some(0), true, &caller_id);
        assert_eq!(who, "test".to_string());
    }

    #[test]
    fn can_get_who_me_custom() {
        let mut options = Options::fake_options(crate::app::export_type::ExportType::Html);
        options.custom_name = Some("Name".to_string());
        let app = Config::fake_app(options);

        let who = app.who(Some(0), true, &None);
        assert_eq!(who, "Name".to_string());
    }

    #[test]
    fn can_get_who_none_me() {
        let options = Options::fake_options(crate::app::export_type::ExportType::Html);
        let app = Config::fake_app(options);

        let who = app.who(None, true, &None);
        assert_eq!(who, "Me".to_string());
    }

    #[test]
    fn can_get_who_me_none_caller_id() {
        let mut options = Options::fake_options(crate::app::export_type::ExportType::Html);
        options.use_caller_id = true;
        let app = Config::fake_app(options);

        let caller_id = Some("test".to_string());
        let who = app.who(None, true, &caller_id);
        assert_eq!(who, "test".to_string());
    }

    #[test]
    fn can_get_who_none_them() {
        let options = Options::fake_options(crate::app::export_type::ExportType::Html);
        let app = Config::fake_app(options);

        let who = app.who(None, false, &None);
        assert_eq!(who, "Unknown".to_string());
    }

    #[test]
    fn can_get_chat_valid() {
        let options = Options::fake_options(crate::app::export_type::ExportType::Html);
        let mut app = Config::fake_app(options);

        let chat = fake_chat();
        app.chatrooms.insert(chat.rowid, chat);
        app.real_chatrooms.insert(0, 0);

        let mut message = Config::fake_message();
        message.chat_id = Some(0);

        let (_, id) = app.conversation(&message).unwrap();
        assert_eq!(id, &0);
    }

    #[test]
    fn can_get_chat_valid_deleted() {
        let options = Options::fake_options(crate::app::export_type::ExportType::Html);
        let mut app = Config::fake_app(options);

        let chat = fake_chat();
        app.chatrooms.insert(chat.rowid, chat);
        app.real_chatrooms.insert(0, 0);

        let mut message = Config::fake_message();
        message.chat_id = None;
        message.deleted_from = Some(0);

        let (_, id) = app.conversation(&message).unwrap();
        assert_eq!(id, &0);
    }

    #[test]
    fn can_get_chat_invalid() {
        let options = Options::fake_options(crate::app::export_type::ExportType::Html);
        let mut app = Config::fake_app(options);

        let chat = fake_chat();
        app.chatrooms.insert(chat.rowid, chat);
        app.real_chatrooms.insert(0, 0);

        let mut message = Config::fake_message();
        message.chat_id = Some(1);

        let room = app.conversation(&message);
        assert!(room.is_none());
    }

    #[test]
    fn can_get_chat_none() {
        let options = Options::fake_options(crate::app::export_type::ExportType::Html);
        let mut app = Config::fake_app(options);

        let chat = fake_chat();
        app.chatrooms.insert(chat.rowid, chat);
        app.real_chatrooms.insert(0, 0);

        let mut message = Config::fake_message();
        message.chat_id = None;
        message.deleted_from = None;

        let room = app.conversation(&message);
        assert!(room.is_none());
    }
}

#[cfg(test)]
mod directory_tests {
    use crate::{Config, Options};
    use std::path::PathBuf;

    #[test]
    fn can_get_valid_attachment_sub_dir() {
        let options = Options::fake_options(crate::app::export_type::ExportType::Html);
        let mut app = Config::fake_app(options);

        app.real_chatrooms.insert(0, 0);

        let sub_dir = app.conversation_attachment_path(Some(0));
        assert_eq!(String::from("0"), sub_dir);
    }

    #[test]
    fn can_get_invalid_attachment_sub_dir() {
        let options = Options::fake_options(crate::app::export_type::ExportType::Html);
        let mut app = Config::fake_app(options);

        app.real_chatrooms.insert(0, 0);

        let sub_dir = app.conversation_attachment_path(Some(1));
        assert_eq!(String::from("orphaned"), sub_dir);
    }

    #[test]
    fn can_get_missing_attachment_sub_dir() {
        let options = Options::fake_options(crate::app::export_type::ExportType::Html);
        let mut app = Config::fake_app(options);

        app.real_chatrooms.insert(0, 0);

        let sub_dir = app.conversation_attachment_path(None);
        assert_eq!(String::from("orphaned"), sub_dir);
    }

    #[test]
    fn can_get_path_not_copied() {
        let options = Options::fake_options(crate::app::export_type::ExportType::Html);
        let app = Config::fake_app(options);

        let attachment = Config::fake_attachment();

        let result = app.message_attachment_path(&attachment);
        let expected = String::from("a/b/c/d.jpg");
        assert_eq!(result, expected);
    }

    #[test]
    fn can_get_path_copied() {
        let mut options = Options::fake_options(crate::app::export_type::ExportType::Html);
        // Set an export path
        options.export_path = PathBuf::from("/Users/ReagentX/exports");

        let app = Config::fake_app(options);

        let mut attachment = Config::fake_attachment();
        let mut full_path = PathBuf::from("/Users/ReagentX/exports/attachments");
        full_path.push(attachment.filename().unwrap());
        attachment.copied_path = Some(full_path);

        let result = app.message_attachment_path(&attachment);
        let expected = String::from("attachments/d.jpg");
        assert_eq!(result, expected);
    }

    #[test]
    fn can_get_path_copied_bad() {
        let mut options = Options::fake_options(crate::app::export_type::ExportType::Html);
        // Set an export path
        options.export_path = PathBuf::from("/Users/ReagentX/exports");

        let app = Config::fake_app(options);

        let mut attachment = Config::fake_attachment();
        attachment.copied_path = Some(PathBuf::from(attachment.filename.as_ref().unwrap()));

        let result = app.message_attachment_path(&attachment);
        let expected = String::from("a/b/c/d.jpg");
        assert_eq!(result, expected);
    }
}

#[cfg(test)]
mod chat_filter_tests {
    use std::collections::BTreeSet;

    use crate::{
        Config, Options,
        app::{contacts::Name, export_type::ExportType},
    };

    #[test]
    fn can_generate_filter_string_multiple() {
        let mut options = Options::fake_options(ExportType::Html);
        options.conversation_filter = Some(String::from("Person 10,Person 11,Person 12"));

        let mut app = Config::fake_app(options);

        // Add some test data
        app.participants.insert(10, Name::fake_name("Person 10")); // Included
        app.participants.insert(11, Name::fake_name("Person 11")); // Included
        app.participants.insert(12, Name::fake_name("Person 12")); // Included
        app.participants.insert(13, Name::fake_name("Person 13")); // Excluded

        // Set the chatroom participant IDs
        for (id, participant) in app.participants.iter_mut() {
            participant.handle_ids.insert(*id);
        }

        // Chatroom 1: Included
        let mut chatroom_1 = BTreeSet::new();
        chatroom_1.insert(10);
        app.chatroom_participants.insert(1, chatroom_1);

        // Chatroom 2: Included
        let mut chatroom_2 = BTreeSet::new();
        chatroom_2.insert(11);
        app.chatroom_participants.insert(2, chatroom_2);

        // Chatroom 3: Included
        let mut chatroom_3 = BTreeSet::new();
        chatroom_3.insert(12);
        app.chatroom_participants.insert(3, chatroom_3);

        // Chatroom 4: Excluded
        let mut chatroom_4 = BTreeSet::new();
        chatroom_4.insert(13);
        app.chatroom_participants.insert(4, chatroom_4);

        // Chatroom 5: Included
        let mut chatroom_5 = BTreeSet::new();
        chatroom_5.insert(10);
        chatroom_5.insert(11);
        app.chatroom_participants.insert(5, chatroom_5);

        // Chatroom 6: Included
        let mut chatroom_6 = BTreeSet::new();
        chatroom_6.insert(12);
        chatroom_6.insert(13); // Even though this person is excluded, the above person is
        app.chatroom_participants.insert(6, chatroom_6);

        app.resolve_filtered_handles();
        // For the test, sort the output so it is always the same

        assert_eq!(
            app.options.query_context.selected_handle_ids,
            Some(BTreeSet::from([10, 11, 12]))
        );
        assert_eq!(
            app.options.query_context.selected_chat_ids,
            Some(BTreeSet::from([1, 2, 3, 5, 6]))
        );
    }

    #[test]
    fn can_generate_filter_string_single() {
        let mut options = Options::fake_options(ExportType::Html);
        options.conversation_filter = Some(String::from("Person 13"));

        let mut app = Config::fake_app(options);

        // Add some test data
        app.participants.insert(10, Name::fake_name("Person 10")); // Excluded
        app.participants.insert(11, Name::fake_name("Person 11")); // Excluded
        app.participants.insert(12, Name::fake_name("Person 12")); // Excluded
        app.participants.insert(13, Name::fake_name("Person 13")); // Included

        // Set the chatroom participant IDs
        for (id, participant) in app.participants.iter_mut() {
            participant.handle_ids.insert(*id);
        }

        // Chatroom 1: Excluded
        let mut chatroom_1 = BTreeSet::new();
        chatroom_1.insert(10);
        app.chatroom_participants.insert(1, chatroom_1);

        // Chatroom 2: Excluded
        let mut chatroom_2 = BTreeSet::new();
        chatroom_2.insert(11);
        app.chatroom_participants.insert(2, chatroom_2);

        // Chatroom 3: Excluded
        let mut chatroom_3 = BTreeSet::new();
        chatroom_3.insert(12);
        app.chatroom_participants.insert(3, chatroom_3);

        // Chatroom 4: Included
        let mut chatroom_4 = BTreeSet::new();
        chatroom_4.insert(13);
        app.chatroom_participants.insert(4, chatroom_4);

        // Chatroom 5: Excluded
        let mut chatroom_5 = BTreeSet::new();
        chatroom_5.insert(10);
        chatroom_5.insert(11);
        app.chatroom_participants.insert(5, chatroom_5);

        // Chatroom 6: Included
        let mut chatroom_6 = BTreeSet::new();
        chatroom_6.insert(12);
        chatroom_6.insert(13); // Even though this person is excluded, the above person is
        app.chatroom_participants.insert(6, chatroom_6);

        app.resolve_filtered_handles();
        // For the test, sort the output so it is always the same

        assert_eq!(
            app.options.query_context.selected_handle_ids,
            Some(BTreeSet::from([13]))
        );
        assert_eq!(
            app.options.query_context.selected_chat_ids,
            Some(BTreeSet::from([4, 6]))
        );
    }
}

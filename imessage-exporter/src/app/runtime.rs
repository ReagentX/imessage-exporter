/*!
 The main app runtime.
*/

use std::{
    cell::RefCell,
    cmp::min,
    collections::{BTreeSet, HashMap, HashSet},
    fs::create_dir_all,
    path::{Path, PathBuf},
};

use fdlimit::raise_fd_limit;
use fs2::available_space;
use imessage_database::{
    tables::{
        attachment::Attachment,
        chat::Chat,
        chat_handle::ChatToHandle,
        handle::Handle,
        messages::Message,
        table::{
            ATTACHMENTS_DIR, Cacheable, Deduplicate, Diagnostic, ME, ORPHANED, UNKNOWN, get_db_size,
        },
    },
    util::{dates::get_offset, size::format_file_size},
};

use crate::{
    Exporter, HTML, TXT,
    app::{
        compatibility::attachment_manager::AttachmentManagerMode, contacts::Name,
        data_source::DataSource, error::RuntimeError, export_type::ExportType, options::Options,
        sanitizers::sanitize_filename,
    },
    exporters::exporter::ATTACHMENT_NO_FILENAME,
};

// Maximum length for filenames
const MAX_LENGTH: usize = 235;

// MARK: Config
/// Stores the application state and handles application lifecycle
pub struct Config {
    /// Map of iMessage database chatroom ID to chatroom information
    pub chatrooms: HashMap<i32, Chat>,
    /// Map of iMessage database chatroom ID to an internal unique chatroom ID
    pub real_chatrooms: HashMap<i32, i32>,
    /// Map of iMessage database chatroom ID to chatroom participant iMessage database handle IDs
    pub chatroom_participants: HashMap<i32, BTreeSet<i32>>,
    /// Map of deduplicated internal participant ID to contact info
    pub participants: HashMap<i32, Name>,
    /// Map of iMessage database handle ID to an internal unique participant ID, used to generate `participants`
    pub real_participants: HashMap<i32, i32>,
    /// Messages that are tapbacks (reactions) to other messages
    pub tapbacks: HashMap<String, HashMap<usize, Vec<Message>>>,
    /// Translated message GUIDs
    pub translated_messages: HashSet<String>,
    /// App configuration options
    pub options: Options,
    /// Global date offset used by the iMessage database:
    pub offset: i64,
    /// Data source for the application
    pub data_source: DataSource,
    /// Tracks generated contact filenames to avoid collisions
    pub contact_filename_usage: RefCell<HashMap<String, usize>>,
    /// Cache of generated filenames per chat rowid to ensure stable filenames across messages
    pub filename_cache: RefCell<HashMap<i32, String>>,
}

impl Config {
    /// Get a deduplicated chat ID or a default value
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

    /// Get the attachment path for the current session
    pub fn attachment_path(&self) -> PathBuf {
        let mut path = self.options.export_path.clone();
        path.push(ATTACHMENTS_DIR);
        path
    }

    /// Get the attachment path for a specific chat ID
    pub fn conversation_attachment_path(&self, chat_id: Option<i32>) -> String {
        if let Some(chat_id) = chat_id
            && let Some(real_id) = self.real_chatrooms.get(&chat_id)
        {
            return real_id.to_string();
        }
        String::from(ORPHANED)
    }

    /// Generate a file path for an attachment
    ///
    /// If the attachment was copied, use that path
    /// if not, default to the filename
    pub fn message_attachment_path(&self, attachment: &Attachment) -> String {
        // Build a relative filepath from the fully qualified one on the `Attachment`
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
                        .unwrap_or(ATTACHMENT_NO_FILENAME)
                        .to_string()
                }),
        }
    }

    /// Get a relative path for the provided file.
    pub fn relative_path(&self, path: &Path) -> String {
        if let Ok(relative_path) = path.strip_prefix(&self.options.export_path) {
            return relative_path.display().to_string();
        }
        path.display().to_string()
    }

    // MARK: Filenames
    /// Get a filename for a chat, possibly using cached data.
    ///
    /// If the chat has an assigned name, use that, truncating if necessary.
    ///
    /// If it does not, first try and make a flat list of its members. Failing that, use the unique `chat_identifier` field.
    pub fn filename(&self, chatroom: &Chat) -> String {
        // Calculate effective max length accounting for export path
        let export_path_len = self.options.export_path.as_os_str().len();
        let max_len = MAX_LENGTH.saturating_sub(export_path_len + 1);

        // Return cached filename if we've already generated one for this chatroom
        if let Some(cached) = self.filename_cache.borrow().get(&chatroom.rowid) {
            return cached.clone();
        }

        let mut filename = match &chatroom.display_name() {
            // If there is a display name, use that
            Some(name) => {
                format!(
                    "{} - {}",
                    &name[..min(max_len, name.len())],
                    // Get the deduplicated chat ID to ensure the filename is unique, even if the group name is not
                    self.real_chatrooms
                        .get(&chatroom.rowid)
                        .unwrap_or(&chatroom.rowid)
                )
            }
            // Fallback if there is no name set
            None => {
                if let Some(participants) = self.chatroom_participants.get(&chatroom.rowid) {
                    let participant_filename = self.filename_from_participants(participants);
                    if self.options.contact_filenames {
                        self.ensure_unique_contact_filename(participant_filename, participants)
                    } else {
                        participant_filename
                    }
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

        let sanitized = sanitize_filename(&filename);
        // Cache the computed filename so subsequent calls return the same value and don't create new files
        self.filename_cache
            .borrow_mut()
            .insert(chatroom.rowid, sanitized.clone());

        sanitized
    }

    /// Generate a filename from a set of participants, truncating if the name is too long
    ///
    /// - All names:
    ///   - Contact 1, Contact 2
    /// - Truncated Names
    ///   - Contact 1, Contact 2, ... Contact 13 and 4 others
    fn filename_from_participants(&self, participants: &BTreeSet<i32>) -> String {
        // Calculate effective max length accounting for export path
        let export_path_len = self.options.export_path.as_os_str().len();
        let max_len = MAX_LENGTH.saturating_sub(export_path_len + 1);

        let mut added = 0;
        let mut out_s = String::with_capacity(max_len);
        for participant_id in participants {
            let participant_details = if self.options.contact_filenames {
                self.contact_filename_label(*participant_id)
                    .unwrap_or_else(|| UNKNOWN.to_string())
            } else {
                match self.resolve_participant(*participant_id) {
                    Some(name) => name.details.clone(),
                    None => UNKNOWN.to_string(),
                }
            };

            if participant_details.len() + out_s.len() < max_len {
                if !out_s.is_empty() {
                    out_s.push_str(", ");
                }
                out_s.push_str(&participant_details);
                added += 1;
            } else {
                let extra = format!(", and {} others", participants.len() - added);
                let space_remaining = extra.len() + out_s.len();
                if space_remaining >= max_len {
                    out_s.replace_range((max_len - extra.len()).., &extra);
                } else if out_s.is_empty() {
                    out_s.push_str(&participant_details[..max_len]);
                } else {
                    out_s.push_str(&extra);
                }
                break;
            }
        }
        out_s
    }

    fn contact_filename_label(&self, participant_id: i32) -> Option<String> {
        let name = self.resolve_participant(participant_id)?;
        clean_contact_name(name.get_display_name()).or_else(|| clean_contact_name(&name.details))
    }

    fn ensure_unique_contact_filename(&self, base: String, participants: &BTreeSet<i32>) -> String {
        let mut usage = self.contact_filename_usage.borrow_mut();
        let counter = usage.entry(base.clone()).or_insert(0);
        let result = if *counter == 0 {
            base.clone()
        } else {
            let suffix = self.contact_filename_suffix(participants);
            if *counter == 1 {
                format!("{base} ({suffix})")
            } else {
                format!("{base} ({suffix}-{counter})")
            }
        };
        *counter += 1;
        result
    }

    fn contact_filename_suffix(&self, participants: &BTreeSet<i32>) -> String {
        for participant_id in participants {
            if let Some(name) = self.resolve_participant(*participant_id) {
                let detail = canonical_handle_detail(&name.details);
                if !detail.is_empty() {
                    let sanitized = sanitize_filename(detail.trim());
                    let trimmed = sanitized.trim();
                    if !trimmed.is_empty() {
                        return trimmed.to_string();
                    }
                }
            }
        }
        UNKNOWN.to_string()
    }

    // MARK: Init
    /// Create a new instance of the application
    ///
    /// # Example:
    ///
    /// ```
    /// use crate::app::{
    ///    options::{from_command_line, Options},
    ///    runtime::Config,
    /// };
    ///
    /// let args = from_command_line();
    /// let options = Options::from_args(&args);
    /// let app = Config::new(options).unwrap();
    /// ```
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
        // Translations are not available in older database versions, so we default to an empty set
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
            contact_filename_usage: RefCell::new(HashMap::new()),
            filename_cache: RefCell::new(HashMap::new()),
        })
    }

    // MARK: Filters
    /// Convert comma separated list of participant strings into table chat IDs using
    ///   1) filter `self.participant` keys based on the values (by comparing to user values)
    ///   2) get the chat IDs keys from `self.chatroom_participants` for values that contain the selected `handle_ids`
    ///   3) send those chat and handle IDs to the query context so they are included in the message table filters
    pub(crate) fn resolve_filtered_handles(&mut self) {
        if let Some(conversation_filter) = &self.options.conversation_filter {
            let parsed_handle_filter = conversation_filter.split(',').collect::<Vec<&str>>();

            let mut included_chatrooms: BTreeSet<i32> = BTreeSet::new();
            let mut included_handles: BTreeSet<i32> = BTreeSet::new();

            // First: Scan the list of participants for included handle IDs
            self.participants.iter().for_each(|(_, handle_name)| {
                for included_name in &parsed_handle_filter {
                    if handle_name.contains(included_name) {
                        included_handles.extend(&handle_name.handle_ids);
                    }
                }
            });

            // Second: scan the list of chatrooms for IDs that contain the selected participants
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

    /// If we set some filtered chatrooms, emit how many will be included in the export
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

    /// Ensure there is available disk space for the requested export
    fn ensure_free_space(&self) -> Result<(), RuntimeError> {
        // Export size is usually about 6% the size of the db;
        // we divide by 10 to over-estimate about 10% of the total size
        // for some safe headroom
        let total_db_size = get_db_size(Path::new(
            self.data_source
                .db()
                .path()
                .ok_or(RuntimeError::FileNameError)?,
        ))?;
        let mut estimated_export_size = total_db_size / 10;

        let free_space_at_location = available_space(&self.options.export_path)?;

        // Validate that there is enough disk space free to write the export
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
                    estimated_export_size + total_attachment_size,
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
    /// Handles diagnostic tests for database
    fn run_diagnostic(&self) -> Result<(), RuntimeError> {
        println!("\niMessage Database Diagnostics\n");
        Handle::run_diagnostic(self.data_source.db())?;
        Message::run_diagnostic(self.data_source.db())?;
        Attachment::run_diagnostic(
            self.data_source.db(),
            &self.options.db_path,
            &self.options.platform,
        )?;
        ChatToHandle::run_diagnostic(self.data_source.db())?;

        // Global Diagnostics
        println!("Global diagnostic data:");

        let total_db_size = get_db_size(Path::new(
            self.data_source
                .db()
                .path()
                .ok_or(RuntimeError::FileNameError)?,
        ))?;
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

        println!(
            "    Handles with resolved names: {}/{} ({}%)",
            total_resolved,
            self.participants.len(),
            (total_resolved as f32 / self.participants.len() as f32 * 100.0).round()
        );

        println!("\nEnvironment Diagnostics\n");
        self.options.attachment_manager.diagnostic();

        Ok(())
    }

    // MARK: Startup
    /// Start the app given the provided set of options. This will either run
    /// diagnostic tests on the database or export data to the specified file type.
    ///
    // # Example:
    ///
    /// ```
    /// use crate::app::{
    ///    options::{from_command_line, Options},
    ///    runtime::Config,
    /// };
    ///
    /// let args = from_command_line();
    /// let options = Options::from_args(&args);
    /// let app = Config::new(options).unwrap();
    /// app.start();
    /// ```
    pub fn start(&self) -> Result<(), RuntimeError> {
        if self.options.diagnostic {
            self.run_diagnostic()?;
        } else if let Some(export_type) = &self.options.export_type {
            // Ensure that if we want to filter on things, we have stuff to filter for
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
                    HTML::new(self)?.iter_messages()?;
                }
                ExportType::Txt => {
                    TXT::new(self)?.iter_messages()?;
                }
            }
        }
        println!("Done!");
        Ok(())
    }

    /// Determine who sent a message
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

    /// Resolve a participant name from a handle ID
    fn resolve_participant(&self, handle_id: i32) -> Option<&Name> {
        if let Some(internal_id) = self.real_participants.get(&handle_id) {
            return self.participants.get(internal_id);
        }
        None
    }
}

fn clean_contact_name(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }

    let without_emoji = remove_emoji(trimmed);
    let without_suffix = strip_professional_suffix(without_emoji.trim());
    let cleaned = without_suffix.split_whitespace().collect::<Vec<_>>().join(" ");
    if cleaned.is_empty() {
        None
    } else {
        Some(cleaned)
    }
}

fn strip_professional_suffix(name: &str) -> &str {
    if let Some(idx) = name.rfind(',') {
        let suffix = name[idx + 1..].trim();
        if !suffix.is_empty() && suffix.chars().all(is_professional_suffix_char) {
            return name[..idx].trim_end();
        }
    }
    name
}

fn is_professional_suffix_char(ch: char) -> bool {
    ch.is_ascii_uppercase()
        || ch == '-'
        || ch == ' '
        || ch == '.'
        || ch == '+'
        || ch.is_ascii_digit()
}

fn canonical_handle_detail(value: &str) -> String {
    let digits: String = value
        .chars()
        .filter(|ch| ch.is_ascii_digit() || *ch == '+')
        .collect();
    if !digits.is_empty() {
        digits
    } else {
        value.trim().to_string()
    }
}

fn remove_emoji(value: &str) -> String {
    value.chars().filter(|ch| !is_emoji(*ch)).collect()
}

fn is_emoji(ch: char) -> bool {
    matches!(
        ch as u32,
        0x1F300..=0x1F5FF
            | 0x1F600..=0x1F64F
            | 0x1F680..=0x1F6FF
            | 0x1F700..=0x1F77F
            | 0x1F900..=0x1F9FF
            | 0x1F1E6..=0x1F1FF
            | 0x2600..=0x26FF
            | 0x2700..=0x27BF
            | 0xFE00..=0xFE0F
    )
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
            contact_filename_usage: RefCell::new(HashMap::new()),
            filename_cache: RefCell::new(HashMap::new()),
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
            components: vec![],
            edited_parts: None,
        }
    }

    pub(crate) fn fake_attachment() -> Attachment {
        Attachment {
            rowid: 0,
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

    use std::collections::{BTreeSet, HashSet};

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

        // Create participant data
        app.participants.insert(10, Name::fake_name("Person 10"));
        app.participants.insert(11, Name::fake_name("Person 11"));
        app.real_participants.insert(10, 10);
        app.real_participants.insert(11, 11);

        // Add participants
        let mut people = BTreeSet::new();
        people.insert(10);
        people.insert(11);

        // Get filename
        let filename = app.filename_from_participants(&people);
        assert_eq!(filename, "Person 10, Person 11".to_string());
        assert!(filename.len() <= MAX_LENGTH);
    }

    #[test]
    fn can_use_contact_names_when_flag_enabled() {
        let mut options = Options::fake_options(crate::app::export_type::ExportType::Html);
        options.contact_filenames = true;
        let mut app = Config::fake_app(options);

        let mut handles = HashSet::new();
        handles.insert(10);
        app.participants.insert(
            10,
            Name {
                first: "Alice".to_string(),
                last: "Smith".to_string(),
                full: "Alice Smith".to_string(),
                details: "5558675309".to_string(),
                handle_ids: handles,
            },
        );
        app.real_participants.insert(10, 10);

        let mut people = BTreeSet::new();
        people.insert(10);

        let filename = app.filename_from_participants(&people);
        assert_eq!(filename, "Alice Smith".to_string());
    }

    #[test]
    fn trims_suffix_and_emoji_from_contact_names() {
        let mut options = Options::fake_options(crate::app::export_type::ExportType::Html);
        options.contact_filenames = true;
        let mut app = Config::fake_app(options);

        let mut handles = HashSet::new();
        handles.insert(10);
        app.participants.insert(
            10,
            Name {
                first: "Elaine".to_string(),
                last: "Vuong".to_string(),
                full: "  Elaine   Vuong, MT-BC  🌟  ".to_string(),
                details: "5558675309".to_string(),
                handle_ids: handles,
            },
        );
        app.real_participants.insert(10, 10);

        let mut people = BTreeSet::new();
        people.insert(10);

        let filename = app.filename_from_participants(&people);
        assert_eq!(filename, "Elaine Vuong".to_string());
    }

    #[test]
    fn keeps_noncomma_suffix_when_in_last_name() {
        let mut options = Options::fake_options(crate::app::export_type::ExportType::Html);
        options.contact_filenames = true;
        let mut app = Config::fake_app(options);

        let mut handles = HashSet::new();
        handles.insert(10);
        app.participants.insert(
            10,
            Name {
                first: "Sarah".to_string(),
                last: "Syzmanowski MT-BC".to_string(),
                full: "Sarah Syzmanowski MT-BC".to_string(),
                details: "5558675309".to_string(),
                handle_ids: handles,
            },
        );
        app.real_participants.insert(10, 10);

        let mut people = BTreeSet::new();
        people.insert(10);

        let filename = app.filename_from_participants(&people);
        assert_eq!(filename, "Sarah Syzmanowski MT-BC".to_string());
    }

    #[test]
    fn appends_phone_suffix_for_duplicate_contact_names() {
        let mut options = Options::fake_options(crate::app::export_type::ExportType::Html);
        options.contact_filenames = true;
        let mut app = Config::fake_app(options);

        let mut handles = HashSet::new();
        handles.insert(10);
        app.participants.insert(
            10,
            Name {
                first: "Sam".to_string(),
                last: "Doe".to_string(),
                full: "Sam Doe".to_string(),
                details: "+15551234567".to_string(),
                handle_ids: handles.clone(),
            },
        );
        app.real_participants.insert(10, 10);

        let mut people = BTreeSet::new();
        people.insert(10);

        let mut chat_one = fake_chat();
        chat_one.rowid = 1;
        let mut chat_two = fake_chat();
        chat_two.rowid = 2;

        app.chatroom_participants
            .insert(chat_one.rowid, people.clone());
        app.chatroom_participants.insert(chat_two.rowid, people);

        let first = app.filename(&chat_one);
        let second = app.filename(&chat_two);
        assert_eq!(first, "Sam Doe.html");
        assert_eq!(second, "Sam Doe (+15551234567).html");
    }

    #[test]
    fn can_get_filename_long_multiple() {
        let options = Options::fake_options(crate::app::export_type::ExportType::Html);
        let mut app = Config::fake_app(options);

        // Create participant data
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

        // Get filename
        let filename = app.filename_from_participants(&people);
        assert_eq!(filename, "Person With An Extremely and Excessively Long Name 10, Person With An Extremely and Excessively Long Name 11, Person With An Extremely and Excessively Long Name 12, Person With An Extremely and Excessively Long Name , and 4 others".to_string());
        assert!(filename.len() <= MAX_LENGTH);
    }

    #[test]
    fn can_get_filename_single_long() {
        let options = Options::fake_options(crate::app::export_type::ExportType::Html);
        let mut app = Config::fake_app(options);

        // Create participant data
        app.participants.insert(10, Name::fake_name("He slipped his key into the lock, and we all very quietly entered the cell. The sleeper half turned, and then settled down once more into a deep slumber. Holmes stooped to the water-jug, moistened his sponge, and then rubbed it twice vigorously across and down the prisoner's face."));
        app.real_participants.insert(10, 10);

        // Add 1 person
        let mut people = BTreeSet::new();
        people.insert(10);

        // Get filename
        let filename = app.filename_from_participants(&people);
        assert_eq!(filename, "He slipped his key into the lock, and we all very quietly entered the cell. The sleeper half turned, and then settled down once more into a deep slumber. Holmes stooped to the water-jug, moistened his sponge, and then rubbed it tw".to_string());
        assert!(filename.len() <= MAX_LENGTH);
    }

    #[test]
    fn can_get_filename_chat_display_name_long() {
        let options = Options::fake_options(crate::app::export_type::ExportType::Html);
        let app = Config::fake_app(options);

        // Create chat
        let mut chat = fake_chat();
        chat.display_name = Some("Life is infinitely stranger than anything which the mind of man could invent. We would not dare to conceive the things which are really mere commonplaces of existence. If we could fly out of that window hand in hand, hover over this great city, gently remove the roofs".to_string());

        // Get filename
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

        // Create chat
        let mut chat = fake_chat();
        chat.display_name = Some("Test Chat Name".to_string());

        // Get filename
        let filename = app.filename(&chat);
        assert_eq!(filename, "Test Chat Name - 0.html");
    }

    #[test]
    fn can_get_filename_chat_display_name_short() {
        let options = Options::fake_options(crate::app::export_type::ExportType::Html);
        let app = Config::fake_app(options);

        // Create chat
        let mut chat = fake_chat();
        chat.display_name = Some("🤠".to_string());

        // Get filename
        let filename = app.filename(&chat);
        assert_eq!(filename, "🤠 - 0.html");
    }

    #[test]
    fn can_get_filename_chat_participants() {
        let options = Options::fake_options(crate::app::export_type::ExportType::Html);
        let mut app = Config::fake_app(options);

        // Create chat
        let chat = fake_chat();

        // Create participant data
        app.participants.insert(10, Name::fake_name("Person 10"));
        app.participants.insert(11, Name::fake_name("Person 11"));
        app.real_participants.insert(10, 10);
        app.real_participants.insert(11, 11);

        // Add participants
        let mut people = BTreeSet::new();
        people.insert(10);
        people.insert(11);
        app.chatroom_participants.insert(chat.rowid, people);

        // Get filename
        let filename = app.filename(&chat);
        assert_eq!(filename, "Person 10, Person 11.html");
    }

    #[test]
    fn can_get_filename_chat_no_participants() {
        let options = Options::fake_options(crate::app::export_type::ExportType::Html);
        let app = Config::fake_app(options);

        // Create chat
        let chat = fake_chat();

        // Get filename
        let filename = app.filename(&chat);
        assert_eq!(filename, "Default.html");
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

        // Create participant data
        app.participants.insert(10, Name::fake_name("Person 10"));
        app.real_participants.insert(10, 10);

        // Get participant name
        let who = app.who(Some(10), false, &None);
        assert_eq!(who, "Person 10".to_string());
    }

    #[test]
    fn can_get_who_them_missing() {
        let options = Options::fake_options(crate::app::export_type::ExportType::Html);
        let app = Config::fake_app(options);

        // Get participant name
        let who = app.who(Some(10), false, &None);
        assert_eq!(who, "Unknown".to_string());
    }

    #[test]
    fn can_get_who_me() {
        let options = Options::fake_options(crate::app::export_type::ExportType::Html);
        let app = Config::fake_app(options);

        // Get participant name
        let who = app.who(Some(0), true, &None);
        assert_eq!(who, "Me".to_string());
    }

    #[test]
    fn can_get_who_me_caller_id() {
        let mut options = Options::fake_options(crate::app::export_type::ExportType::Html);
        options.use_caller_id = true;
        let app = Config::fake_app(options);

        // Get participant name
        let caller_id = Some("test".to_string());
        let who = app.who(Some(0), true, &caller_id);
        assert_eq!(who, "test".to_string());
    }

    #[test]
    fn can_get_who_me_custom() {
        let mut options = Options::fake_options(crate::app::export_type::ExportType::Html);
        options.custom_name = Some("Name".to_string());
        let app = Config::fake_app(options);

        // Get participant name
        let who = app.who(Some(0), true, &None);
        assert_eq!(who, "Name".to_string());
    }

    #[test]
    fn can_get_who_none_me() {
        let options = Options::fake_options(crate::app::export_type::ExportType::Html);
        let app = Config::fake_app(options);

        // Get participant name
        let who = app.who(None, true, &None);
        assert_eq!(who, "Me".to_string());
    }

    #[test]
    fn can_get_who_me_none_caller_id() {
        let mut options = Options::fake_options(crate::app::export_type::ExportType::Html);
        options.use_caller_id = true;
        let app = Config::fake_app(options);

        // Get participant name
        let caller_id = Some("test".to_string());
        let who = app.who(None, true, &caller_id);
        assert_eq!(who, "test".to_string());
    }

    #[test]
    fn can_get_who_none_them() {
        let options = Options::fake_options(crate::app::export_type::ExportType::Html);
        let app = Config::fake_app(options);

        // Get participant name
        let who = app.who(None, false, &None);
        assert_eq!(who, "Unknown".to_string());
    }

    #[test]
    fn can_get_chat_valid() {
        let options = Options::fake_options(crate::app::export_type::ExportType::Html);
        let mut app = Config::fake_app(options);

        // Create chat
        let chat = fake_chat();
        app.chatrooms.insert(chat.rowid, chat);
        app.real_chatrooms.insert(0, 0);

        // Create message
        let mut message = Config::fake_message();
        message.chat_id = Some(0);

        // Get filename
        let (_, id) = app.conversation(&message).unwrap();
        assert_eq!(id, &0);
    }

    #[test]
    fn can_get_chat_valid_deleted() {
        let options = Options::fake_options(crate::app::export_type::ExportType::Html);
        let mut app = Config::fake_app(options);

        // Create chat
        let chat = fake_chat();
        app.chatrooms.insert(chat.rowid, chat);
        app.real_chatrooms.insert(0, 0);

        // Create message
        let mut message = Config::fake_message();
        message.chat_id = None;
        message.deleted_from = Some(0);

        // Get filename
        let (_, id) = app.conversation(&message).unwrap();
        assert_eq!(id, &0);
    }

    #[test]
    fn can_get_chat_invalid() {
        let options = Options::fake_options(crate::app::export_type::ExportType::Html);
        let mut app = Config::fake_app(options);

        // Create chat
        let chat = fake_chat();
        app.chatrooms.insert(chat.rowid, chat);
        app.real_chatrooms.insert(0, 0);

        // Create message
        let mut message = Config::fake_message();
        message.chat_id = Some(1);

        // Get filename
        let room = app.conversation(&message);
        assert!(room.is_none());
    }

    #[test]
    fn can_get_chat_none() {
        let options = Options::fake_options(crate::app::export_type::ExportType::Html);
        let mut app = Config::fake_app(options);

        // Create chat
        let chat = fake_chat();
        app.chatrooms.insert(chat.rowid, chat);
        app.real_chatrooms.insert(0, 0);

        // Create message
        let mut message = Config::fake_message();
        message.chat_id = None;
        message.deleted_from = None;

        // Get filename
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

        // Create chatroom ID
        app.real_chatrooms.insert(0, 0);

        // Get subdirectory
        let sub_dir = app.conversation_attachment_path(Some(0));
        assert_eq!(String::from("0"), sub_dir);
    }

    #[test]
    fn can_get_invalid_attachment_sub_dir() {
        let options = Options::fake_options(crate::app::export_type::ExportType::Html);
        let mut app = Config::fake_app(options);

        // Create chatroom ID
        app.real_chatrooms.insert(0, 0);

        // Get subdirectory
        let sub_dir = app.conversation_attachment_path(Some(1));
        assert_eq!(String::from("orphaned"), sub_dir);
    }

    #[test]
    fn can_get_missing_attachment_sub_dir() {
        let options = Options::fake_options(crate::app::export_type::ExportType::Html);
        let mut app = Config::fake_app(options);

        // Create chatroom ID
        app.real_chatrooms.insert(0, 0);

        // Get subdirectory
        let sub_dir = app.conversation_attachment_path(None);
        assert_eq!(String::from("orphaned"), sub_dir);
    }

    #[test]
    fn can_get_path_not_copied() {
        let options = Options::fake_options(crate::app::export_type::ExportType::Html);
        let app = Config::fake_app(options);

        // Create attachment
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

        // Create attachment
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

        // Create attachment
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

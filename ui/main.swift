import AppKit
import SQLite3

/// One row in the conversation picker.
struct Chat {
    let title: String      // shown in the picker
    let filterValue: String // passed to `-t`
}

/// Minimal native front-end for the `imessage-exporter` CLI.
final class ExporterUI: NSObject, NSApplicationDelegate, NSComboBoxDataSource {
    private let window = NSWindow(
        contentRect: NSRect(x: 0, y: 0, width: 700, height: 640),
        styleMask: [.titled, .closable, .miniaturizable, .resizable],
        backing: .buffered, defer: false)
    private let defaults = UserDefaults.standard

    private let binaryPath = NSTextField()
    private let exportPath = NSTextField()
    private let chatPicker = NSComboBox()
    private let startDate = NSTextField()
    private let endDate = NSTextField()
    private let format = NSSegmentedControl()
    private let copyMethod = NSPopUpButton()
    private let output = NSTextView()
    private let runButton = NSButton(title: "Export", target: nil, action: nil)
    private let spinner = NSProgressIndicator()
    private let statusLabel = NSTextField(labelWithString: "")
    private var process: Process?
    private var chats: [Chat] = []

    /// Resolve the CLI binary: an absolute path is used as-is; a bare name is
    /// looked up on PATH (via `which`). Returns nil when not found.
    private func resolveBinary(_ value: String) -> String? {
        if value.contains("/") {
            return FileManager.default.isExecutableFile(atPath: value) ? value : nil
        }
        let which = Process()
        which.executableURL = URL(fileURLWithPath: "/usr/bin/which")
        which.arguments = [value]
        let pipe = Pipe()
        which.standardOutput = pipe
        guard let _ = try? which.run() else { return nil }
        which.waitUntilExit()
        let path = String(data: pipe.fileHandleForReading.readDataToEndOfFile(), encoding: .utf8)?
            .trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        return which.terminationStatus == 0 && !path.isEmpty ? path : nil
    }

    // MARK: Chat list

    /// Load conversations from chat.db, most recent first. Read-only; failures
    /// leave the picker empty (free text still works).
    private func loadChats() -> [Chat] {
        var db: OpaquePointer?
        let path = NSHomeDirectory() + "/Library/Messages/chat.db"
        guard sqlite3_open_v2(path, &db, SQLITE_OPEN_READONLY, nil) == SQLITE_OK else { return [] }
        defer { sqlite3_close(db) }

        let sql = """
            SELECT c.ROWID, c.chat_identifier, c.display_name, COUNT(j.message_id)
            FROM chat c LEFT JOIN chat_message_join j ON j.chat_id = c.ROWID
            LEFT JOIN message m ON m.ROWID = j.message_id
            GROUP BY c.ROWID ORDER BY MAX(m.date) DESC
            """
        var stmt: OpaquePointer?
        guard sqlite3_prepare_v2(db, sql, -1, &stmt, nil) == SQLITE_OK else { return [] }
        defer { sqlite3_finalize(stmt) }

        var result: [Chat] = []
        while sqlite3_step(stmt) == SQLITE_ROW {
            let rowid = sqlite3_column_int64(stmt, 0)
            let identifier = sqlite3_column_text(stmt, 1).map { String(cString: $0) } ?? ""
            let display = sqlite3_column_text(stmt, 2).map { String(cString: $0) } ?? ""
            let count = sqlite3_column_int(stmt, 3)

            // Group chats: filter by every participant so `-t` matches.
            var filterValue = identifier
            if identifier.hasPrefix("chat") {
                var hStmt: OpaquePointer?
                let hSQL = """
                    SELECT h.id FROM handle h
                    JOIN chat_handle_join j ON j.handle_id = h.ROWID WHERE j.chat_id = ?
                    """
                if sqlite3_prepare_v2(db, hSQL, -1, &hStmt, nil) == SQLITE_OK {
                    sqlite3_bind_int64(hStmt, 1, rowid)
                    var members: [String] = []
                    while sqlite3_step(hStmt) == SQLITE_ROW {
                        if let text = sqlite3_column_text(hStmt, 0) { members.append(String(cString: text)) }
                    }
                    if !members.isEmpty { filterValue = members.joined(separator: ",") }
                }
                sqlite3_finalize(hStmt)
            }

            let name = display.isEmpty
                ? (identifier.hasPrefix("chat") ? "Group chat" : identifier)
                : display
            let title = count > 0 ? "\(name)  ·  \(count) msgs" : name
            result.append(Chat(title: title, filterValue: filterValue))
        }
        return result
    }

    // MARK: App lifecycle

    func applicationDidFinishLaunching(_: Notification) {
        window.title = "iMessage Exporter"
        window.center()
        window.minSize = NSSize(width: 640, height: 560)

        // Controls
        binaryPath.stringValue = defaults.string(forKey: "binaryPath") ?? "imessage-exporter"
        binaryPath.toolTip = "Path to the imessage-exporter CLI binary, or just its name if it is on PATH"
        exportPath.stringValue = defaults.string(forKey: "exportPath")
            ?? NSHomeDirectory() + "/imessage_export"
        startDate.placeholderString = "YYYY-MM-DD"
        endDate.placeholderString = "YYYY-MM-DD"
        startDate.stringValue = defaults.string(forKey: "startDate") ?? ""
        endDate.stringValue = defaults.string(forKey: "endDate") ?? ""

        chatPicker.usesDataSource = true
        chatPicker.dataSource = self
        chatPicker.completes = true
        chatPicker.placeholderString = "All conversations"
        chatPicker.toolTip = "Pick a conversation, or type a name, email, or phone number. Empty = export everything."

        format.segmentCount = 2
        format.setLabel("HTML (browsable)", forSegment: 0)
        format.setLabel("Plain text", forSegment: 1)
        format.selectedSegment = defaults.integer(forKey: "format") // 0 default

        copyMethod.addItems(withTitles: [
            "Clone — exact copies of originals (recommended for archiving)",
            "Basic — clone + convert HEIC photos to JPEG",
            "Full — clone + convert HEIC, CAF, MOV to JPEG/MP4",
            "Disabled — reference source files only",
        ])
        copyMethod.selectItem(at: defaults.object(forKey: "copyMethod") == nil ? 0 : defaults.integer(forKey: "copyMethod"))

        let choose = NSButton(title: "Choose…", target: self, action: #selector(chooseFolder))
        let diagnostics = NSButton(title: "Diagnostics", target: self, action: #selector(runDiagnostics))
        diagnostics.toolTip = "Report database stats without exporting"
        let openFolder = NSButton(title: "Open Export Folder", target: self, action: #selector(openExportFolder))
        runButton.target = self
        runButton.action = #selector(runExport)
        runButton.keyEquivalent = "\r"
        runButton.bezelStyle = .rounded

        spinner.style = .spinning
        spinner.controlSize = .small
        spinner.isDisplayedWhenStopped = false
        statusLabel.textColor = .secondaryLabelColor
        statusLabel.lineBreakMode = .byTruncatingTail

        output.isEditable = false
        output.font = .monospacedSystemFont(ofSize: 11, weight: .regular)
        let scroll = NSScrollView()
        scroll.documentView = output
        scroll.hasVerticalScroller = true

        // Layout
        func labeled(_ label: String, _ field: NSView) -> NSStackView {
            let l = NSTextField(labelWithString: label)
            l.alignment = .right
            l.widthAnchor.constraint(equalToConstant: 100).isActive = true
            let row = NSStackView(views: [l, field])
            row.orientation = .horizontal
            return row
        }

        let buttons = NSStackView(views: [runButton, diagnostics, openFolder, spinner, statusLabel])
        buttons.orientation = .horizontal
        buttons.spacing = 10
        statusLabel.widthAnchor.constraint(greaterThanOrEqualToConstant: 160).isActive = true

        let form = NSStackView(views: [
            labeled("Conversation:", chatPicker),
            labeled("Format:", format),
            labeled("Attachments:", copyMethod),
            labeled("From date:", startDate),
            labeled("To date:", endDate),
            labeled("Export folder:", exportPath),
            labeled("", choose),
            labeled("CLI binary:", binaryPath),
            buttons,
            scroll,
        ])
        form.orientation = .vertical
        form.alignment = .leading
        form.spacing = 10
        form.edgeInsets = NSEdgeInsets(top: 18, left: 18, bottom: 18, right: 18)
        form.translatesAutoresizingMaskIntoConstraints = false
        scroll.heightAnchor.constraint(greaterThanOrEqualToConstant: 280).isActive = true

        let content = window.contentView!
        content.addSubview(form)
        NSLayoutConstraint.activate([
            form.leadingAnchor.constraint(equalTo: content.leadingAnchor),
            form.trailingAnchor.constraint(equalTo: content.trailingAnchor),
            form.topAnchor.constraint(equalTo: content.topAnchor),
            form.bottomAnchor.constraint(equalTo: content.bottomAnchor),
            scroll.widthAnchor.constraint(equalTo: form.widthAnchor, constant: -36),
        ])
        for view in [exportPath, startDate, endDate] as [NSTextField] {
            view.widthAnchor.constraint(greaterThanOrEqualToConstant: 460).isActive = true
        }
        chatPicker.widthAnchor.constraint(greaterThanOrEqualToConstant: 460).isActive = true

        window.makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps: true)

        // Load the conversation list in the background.
        statusLabel.stringValue = "Loading conversations…"
        DispatchQueue.global(qos: .userInitiated).async {
            let loaded = self.loadChats()
            DispatchQueue.main.async {
                self.chats = loaded
                self.chatPicker.reloadData()
                self.statusLabel.stringValue = loaded.isEmpty
                    ? "Conversation list unavailable — free text still works"
                    : "\(loaded.count) conversations"
            }
        }
    }

    // MARK: NSComboBoxDataSource

    func numberOfItems(in _: NSComboBox) -> Int { chats.count }
    func comboBox(_: NSComboBox, objectValueForItemAt index: Int) -> Any? { chats[index].title }

    // MARK: Actions

    @objc private func chooseFolder() {
        let panel = NSOpenPanel()
        panel.canChooseDirectories = true
        panel.canChooseFiles = false
        if panel.runModal() == .OK, let url = panel.url {
            exportPath.stringValue = url.path
        }
    }

    @objc private func openExportFolder() {
        NSWorkspace.shared.open(URL(fileURLWithPath: exportPath.stringValue))
    }

    @objc private func runDiagnostics() { run(extra: ["--diagnostics"]) }

    @objc private func runExport() {
        var args = ["-f", format.selectedSegment == 1 ? "txt" : "html",
                    "-c", ["clone", "basic", "full", "disabled"][copyMethod.indexOfSelectedItem],
                    "-o", exportPath.stringValue,
                    "--no-progress"]
        let picked = chatPicker.stringValue.trimmingCharacters(in: .whitespaces)
        if !picked.isEmpty {
            // Use the underlying identifiers when a picker row was chosen as-is.
            let value = chats.first(where: { $0.title == picked })?.filterValue ?? picked
            args += ["-t", value]
        }
        if !startDate.stringValue.isEmpty { args += ["-s", startDate.stringValue] }
        if !endDate.stringValue.isEmpty { args += ["-e", endDate.stringValue] }
        run(extra: args)
    }

    private func run(extra: [String]) {
        guard process == nil else { append("Already running.\n"); return }
        guard let binary = resolveBinary(binaryPath.stringValue) else {
            append("Binary not found: \(binaryPath.stringValue)\nSet the path to the imessage-exporter CLI.\n"); return
        }
        saveSettings()

        let proc = Process()
        proc.executableURL = URL(fileURLWithPath: binary)
        proc.arguments = extra
        let pipe = Pipe()
        proc.standardOutput = pipe
        proc.standardError = pipe
        pipe.fileHandleForReading.readabilityHandler = { handle in
            let data = handle.availableData
            guard !data.isEmpty, let text = String(data: data, encoding: .utf8) else { return }
            DispatchQueue.main.async { self.append(text) }
        }
        proc.terminationHandler = { proc in
            DispatchQueue.main.async {
                self.append("\n--- Exited with code \(proc.terminationStatus) ---\n")
                self.statusLabel.stringValue = proc.terminationStatus == 0
                    ? "Export complete"
                    : "Incomplete — see log (missing attachments are not on disk)"
                self.spinner.stopAnimation(nil)
                self.process = nil
                self.runButton.isEnabled = true
            }
        }
        do {
            try proc.run()
            process = proc
            runButton.isEnabled = false
            spinner.startAnimation(nil)
            statusLabel.stringValue = "Running…"
            append("$ imessage-exporter \(extra.joined(separator: " "))\n\n")
        } catch {
            append("Failed to launch: \(error.localizedDescription)\n")
        }
    }

    private func saveSettings() {
        defaults.set(binaryPath.stringValue, forKey: "binaryPath")
        defaults.set(exportPath.stringValue, forKey: "exportPath")
        defaults.set(startDate.stringValue, forKey: "startDate")
        defaults.set(endDate.stringValue, forKey: "endDate")
        defaults.set(format.selectedSegment, forKey: "format")
        defaults.set(copyMethod.indexOfSelectedItem, forKey: "copyMethod")
    }

    private func append(_ text: String) {
        output.textStorage?.append(NSAttributedString(string: text))
        output.scrollToEndOfDocument(nil)
    }

    func applicationShouldTerminateAfterLastWindowClosed(_: NSApplication) -> Bool { true }
}

let app = NSApplication.shared
let delegate = ExporterUI()
app.delegate = delegate
app.setActivationPolicy(.regular)
app.run()

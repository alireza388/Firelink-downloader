import AppKit
import SwiftUI

struct DownloadPropertiesWindow: View {
    @EnvironmentObject private var controller: DownloadController
    let downloadID: UUID

    var body: some View {
        if let item = controller.downloads.first(where: { $0.id == downloadID }) {
            DownloadPropertiesView(item: item)
        } else {
            ContentUnavailableView("Download Not Found", systemImage: "questionmark.circle")
                .frame(width: 420, height: 240)
        }
    }
}

struct DownloadPropertiesView: View {
    enum LoginMode: String, CaseIterable, Identifiable {
        case matching = "Matching site login"
        case custom = "Custom credentials"
        case none = "No login"

        var id: String { rawValue }
    }

    @EnvironmentObject private var controller: DownloadController
    @EnvironmentObject private var settings: AppSettings
    @Environment(\.dismiss) private var dismiss

    let item: DownloadItem

    @State private var urlText: String
    @State private var fileName: String
    @State private var destinationPath: String
    @State private var connections: Int
    @State private var loginMode: LoginMode
    @State private var username: String
    @State private var password: String
    @State private var errorMessage = ""

    init(item: DownloadItem) {
        self.item = item
        _urlText = State(initialValue: item.url.absoluteString)
        _fileName = State(initialValue: item.fileName)
        _destinationPath = State(initialValue: item.destinationDirectory.path)
        _connections = State(initialValue: item.connectionsPerServer)
        if let credentials = item.credentials {
            _loginMode = State(initialValue: .custom)
            _username = State(initialValue: credentials.username)
            _password = State(initialValue: credentials.password)
        } else {
            _loginMode = State(initialValue: .matching)
            _username = State(initialValue: "")
            _password = State(initialValue: "")
        }
    }

    var body: some View {
        VStack(spacing: 0) {
            Form {
                Section("Download") {
                    TextField("URL", text: $urlText)
                        .font(.system(.body, design: .monospaced))
                    TextField("File name", text: $fileName)
                    HStack {
                        TextField("Save location", text: $destinationPath)
                            .font(.system(.body, design: .monospaced))
                        Button {
                            selectDestination()
                        } label: {
                            Label("Select", systemImage: "folder.badge.plus")
                        }
                    }
                    Stepper("Connections per file: \(connections)", value: $connections, in: 1...16)
                }

                Section("Site Login") {
                    Picker("Login", selection: $loginMode) {
                        ForEach(LoginMode.allCases) { mode in
                            Text(mode.rawValue).tag(mode)
                        }
                    }
                    .pickerStyle(.segmented)

                    if loginMode == .matching {
                        Text(matchingLoginText)
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    } else if loginMode == .custom {
                        TextField("Username", text: $username)
                        SecureField("Password", text: $password)
                    }
                }

                Section("Progress") {
                    ProgressView(value: item.progress)
                    InfoGrid(item: item)
                }
            }
            .formStyle(.grouped)

            Divider()
            HStack {
                Text(errorMessage)
                    .font(.caption)
                    .foregroundStyle(.red)
                    .lineLimit(1)
                Spacer()
                Button("Cancel") {
                    dismiss()
                }
                Button {
                    save()
                } label: {
                    Label("Save", systemImage: "checkmark")
                }
                .buttonStyle(.borderedProminent)
            }
            .padding(14)
            .background(.bar)
        }
        .frame(width: 620, height: 560)
    }

    private var matchingLoginText: String {
        guard let url = URL(string: urlText),
              let credentials = settings.credentials(for: url) else {
            return "No matching saved login for this URL."
        }

        return "Will use saved login for \(credentials.username)."
    }

    private func selectDestination() {
        let panel = NSOpenPanel()
        panel.canChooseFiles = false
        panel.canChooseDirectories = true
        panel.allowsMultipleSelection = false
        panel.canCreateDirectories = true
        panel.directoryURL = URL(fileURLWithPath: NSString(string: destinationPath).expandingTildeInPath)

        if panel.runModal() == .OK, let url = panel.url {
            destinationPath = url.path
        }
    }

    private func save() {
        guard let url = URL(string: urlText.trimmingCharacters(in: .whitespacesAndNewlines)),
              let scheme = url.scheme?.lowercased(),
              ["http", "https", "ftp", "sftp"].contains(scheme) else {
            errorMessage = "Enter a valid HTTP, HTTPS, FTP, or SFTP URL."
            return
        }

        let cleanFileName = fileName.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !cleanFileName.isEmpty else {
            errorMessage = "File name cannot be empty."
            return
        }

        let destination = URL(
            fileURLWithPath: NSString(string: destinationPath.trimmingCharacters(in: .whitespacesAndNewlines)).expandingTildeInPath,
            isDirectory: true
        )

        let credentials: DownloadCredentials?
        switch loginMode {
        case .matching:
            credentials = settings.credentials(for: url)
        case .custom:
            let custom = DownloadCredentials(username: username, password: password)
            credentials = custom.isEmpty ? nil : custom
        case .none:
            credentials = nil
        }

        controller.updateDownload(
            id: item.id,
            url: url,
            fileName: cleanFileName,
            destinationDirectory: destination,
            connectionsPerServer: connections,
            credentials: credentials
        )
        dismiss()
    }
}

private struct InfoGrid: View {
    let item: DownloadItem

    var body: some View {
        Grid(alignment: .leading, horizontalSpacing: 18, verticalSpacing: 8) {
            info("Status", item.status.rawValue)
            info("Progress", item.progress.formatted(.percent.precision(.fractionLength(0))))
            info("Size", ByteFormatter.string(item.sizeBytes))
            info("Speed", item.speedText)
            info("ETA", item.etaText)
            info("Live connections", "\(item.connectionCount)")
            info("Date added", item.createdAt.formatted(date: .abbreviated, time: .shortened))
            info("Last try", item.lastTryAt?.formatted(date: .abbreviated, time: .shortened) ?? "-")
            info("Category", item.category.rawValue)
            info("Destination", item.destinationPath)
        }
    }

    private func info(_ label: String, _ value: String) -> some View {
        GridRow {
            Text(label)
                .foregroundStyle(.secondary)
            Text(value)
                .lineLimit(2)
        }
    }
}

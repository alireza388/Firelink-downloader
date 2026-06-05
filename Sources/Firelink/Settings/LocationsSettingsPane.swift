import AppKit
import SwiftUI

struct LocationsSettingsPane: View {
    @EnvironmentObject private var settings: AppSettings

    var body: some View {
        Form {
            Section {
                ForEach(DownloadCategory.allCases, id: \.self) { category in
                    DirectoryPickerRow(category: category)
                }

                HStack {
                    Spacer()
                    Button("Reset Defaults") {
                        settings.resetDirectories()
                    }
                }
            }
        }
        .formStyle(.grouped)
    }
}

struct DirectoryPickerRow: View {
    @EnvironmentObject private var settings: AppSettings
    let category: DownloadCategory

    @State private var path = ""
    @State private var message = ""

    var body: some View {
        LabeledContent {
            VStack(alignment: .leading, spacing: 4) {
                HStack(spacing: 8) {
                    TextField("Folder path", text: $path)
                        .textFieldStyle(.roundedBorder)
                        .font(.system(.body, design: .monospaced))
                        .onSubmit {
                            applyPath()
                        }

                    Button {
                        applyPath()
                    } label: {
                        Label("Apply", systemImage: "checkmark")
                    }
                    .disabled(path.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)

                    Button {
                        selectFolder()
                    } label: {
                        Label("Select", systemImage: "folder.badge.plus")
                    }
                }

                Text(message.isEmpty ? statusMessage(for: path) : message)
                    .font(.caption)
                    .foregroundStyle(message.isEmpty ? .secondary : .primary)
            }
        } label: {
            Label(category.rawValue, systemImage: category.symbolName)
        }
        .onAppear {
            syncPathFromSettings()
        }
        .onChange(of: settings.downloadDirectories[category]) { _, _ in
            syncPathFromSettings()
        }
    }

    private func selectFolder() {
        let panel = NSOpenPanel()
        panel.canChooseFiles = false
        panel.canChooseDirectories = true
        panel.allowsMultipleSelection = false
        panel.canCreateDirectories = true
        panel.directoryURL = settings.destinationDirectory(for: category)

        if panel.runModal() == .OK, let url = panel.url {
            path = url.path
            settings.setDirectory(url.path, for: category)
            message = "Saved."
        }
    }

    private func syncPathFromSettings() {
        path = settings.downloadDirectories[category] ?? settings.destinationDirectory(for: category).path
        message = ""
    }

    private func applyPath() {
        let trimmed = path.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else {
            message = "Enter a folder path."
            return
        }

        let expanded = NSString(string: trimmed).expandingTildeInPath
        var isDirectory: ObjCBool = false

        if FileManager.default.fileExists(atPath: expanded, isDirectory: &isDirectory) {
            guard isDirectory.boolValue else {
                message = "This path points to a file, not a folder."
                return
            }
        } else {
            do {
                try FileManager.default.createDirectory(
                    at: URL(fileURLWithPath: expanded, isDirectory: true),
                    withIntermediateDirectories: true
                )
            } catch {
                message = "Could not create folder: \(error.localizedDescription)"
                return
            }
        }

        guard FileManager.default.isWritableFile(atPath: expanded) else {
            message = "Firelink cannot write to this folder."
            return
        }

        settings.setDirectory(expanded, for: category)
        path = expanded
        message = "Saved."
    }

    private func statusMessage(for path: String) -> String {
        let trimmed = path.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return "Enter a folder path." }

        let expanded = NSString(string: trimmed).expandingTildeInPath
        var isDirectory: ObjCBool = false

        if FileManager.default.fileExists(atPath: expanded, isDirectory: &isDirectory) {
            if !isDirectory.boolValue {
                return "This path points to a file, not a folder."
            }
            return FileManager.default.isWritableFile(atPath: expanded)
                ? "Ready."
                : "Firelink cannot write to this folder."
        }

        return "Folder will be created when applied."
    }
}

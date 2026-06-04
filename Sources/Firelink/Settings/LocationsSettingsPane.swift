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

    var body: some View {
        LabeledContent {
            HStack(spacing: 8) {
                TextField("Folder path", text: Binding(
                    get: { settings.downloadDirectories[category] ?? path },
                    set: { newValue in
                        path = newValue
                        settings.setDirectory(newValue, for: category)
                    }
                ))
                .textFieldStyle(.roundedBorder)
                .font(.system(.body, design: .monospaced))

                Button {
                    selectFolder()
                } label: {
                    Label("Select", systemImage: "folder.badge.plus")
                }
            }
        } label: {
            Label(category.rawValue, systemImage: category.symbolName)
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
            settings.setDirectory(url.path, for: category)
        }
    }
}

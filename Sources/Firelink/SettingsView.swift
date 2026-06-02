import SwiftUI

private enum SettingsSection: String, CaseIterable, Hashable {
    case downloads = "Downloads"
    case locations = "Locations"
    case siteLogins = "Site Logins"
    case power = "Power"

    var symbolName: String {
        switch self {
        case .downloads: "arrow.down.circle"
        case .locations: "folder"
        case .siteLogins: "key.fill"
        case .power: "moon.zzz"
        }
    }
}

struct SettingsView: View {
    @State private var selection: SettingsSection = .downloads

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Text("Settings")
                .font(.largeTitle.weight(.semibold))
                .padding(.horizontal, 28)
                .padding(.top, 26)
                .padding(.bottom, 18)

            Divider()

            HStack(spacing: 0) {
                settingsSidebar

                Divider()

                ScrollView {
                    selectedPane
                        .frame(maxWidth: 720, alignment: .leading)
                        .padding(28)
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
    }

    private var settingsSidebar: some View {
        List(selection: $selection) {
            Section("Preferences") {
                ForEach(SettingsSection.allCases, id: \.self) { section in
                    Label(section.rawValue, systemImage: section.symbolName)
                        .tag(section)
                }
            }
        }
        .listStyle(.sidebar)
        .frame(width: 210)
    }

    @ViewBuilder
    private var selectedPane: some View {
        switch selection {
        case .downloads:
            DownloadSettingsPane()
        case .locations:
            LocationsSettingsPane()
        case .siteLogins:
            SiteLoginsSettingsPane()
        case .power:
            PowerSettingsPane()
        }
    }
}

private struct DownloadSettingsPane: View {
    @EnvironmentObject private var settings: AppSettings

    var body: some View {
        Form {
            Section {
                Stepper(
                    "Default connections per server: \(settings.perServerConnections)",
                    value: $settings.perServerConnections,
                    in: 1...16
                )
                Text("Used as the default for new downloads. The Add Downloads window can override it per batch.")
                    .font(.caption)
                    .foregroundStyle(.secondary)

                Stepper(
                    "Parallel downloads: \(settings.maxConcurrentDownloads)",
                    value: $settings.maxConcurrentDownloads,
                    in: 1...12
                )
                Text("Controls how many files Firelink downloads at the same time.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
        }
        .formStyle(.grouped)
    }
}

private struct LocationsSettingsPane: View {
    @EnvironmentObject private var settings: AppSettings

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
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
}

private struct DirectoryPickerRow: View {
    @EnvironmentObject private var settings: AppSettings
    let category: DownloadCategory

    @State private var path = ""

    var body: some View {
        HStack(spacing: 10) {
            Label(category.rawValue, systemImage: category.symbolName)
                .frame(width: 125, alignment: .leading)

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

private struct SiteLoginsSettingsPane: View {
    @EnvironmentObject private var settings: AppSettings
    @State private var urlPattern = ""
    @State private var username = ""
    @State private var password = ""

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            Grid(alignment: .leading, horizontalSpacing: 10, verticalSpacing: 10) {
                GridRow {
                    Text("URL Pattern")
                    TextField("*.github.com", text: $urlPattern)
                        .textFieldStyle(.roundedBorder)
                }

                GridRow {
                    Text("Username")
                    TextField("Username", text: $username)
                        .textFieldStyle(.roundedBorder)
                }

                GridRow {
                    Text("Password")
                    SecureField("Password", text: $password)
                        .textFieldStyle(.roundedBorder)
                }
            }

            HStack {
                Text(settings.message)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                Spacer()
                Button {
                    settings.addSiteLogin(urlPattern: urlPattern, username: username, password: password)
                    if settings.message.hasPrefix("Added") {
                        urlPattern = ""
                        username = ""
                        password = ""
                    }
                } label: {
                    Label("Add Login", systemImage: "plus")
                }
                .buttonStyle(.borderedProminent)
            }

            List {
                ForEach(settings.siteLogins) { login in
                    HStack {
                        Image(systemName: "key.horizontal")
                            .foregroundStyle(.secondary)
                        Text(login.urlPattern)
                            .font(.system(.body, design: .monospaced))
                        Spacer()
                        Text(login.username)
                            .foregroundStyle(.secondary)
                    }
                }
                .onDelete(perform: settings.deleteSiteLogins)
            }
            .frame(minHeight: 180)
        }
    }
}

private struct PowerSettingsPane: View {
    @EnvironmentObject private var settings: AppSettings

    var body: some View {
        Form {
            Section {
                Toggle("Prevent system sleep while downloads are active", isOn: $settings.preventsSleepWhileDownloading)
                Text("The display may still turn off. Firelink only keeps macOS awake enough to finish active downloads.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
        }
        .formStyle(.grouped)
    }
}

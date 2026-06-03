import AppKit
import SwiftUI

private enum SettingsSection: String, CaseIterable, Hashable {
    case downloads = "Downloads"
    case locations = "Locations"
    case lookAndFeel = "Look and feel"
    case network = "Network"
    case siteLogins = "Site Logins"
    case power = "Power"
    case engine = "Engine"
    case integration = "Integrations"
    case about = "About"

    static let orderedCases: [SettingsSection] = [
        .downloads,
        .locations,
        .lookAndFeel,
        .network,
        .siteLogins,
        .power,
        .engine,
        .integration,
        .about
    ]

    var symbolName: String {
        switch self {
        case .downloads: "arrow.down.circle"
        case .lookAndFeel: "paintpalette"
        case .network: "network"
        case .locations: "folder"
        case .siteLogins: "key.fill"
        case .power: "moon.zzz"
        case .engine: "terminal"
        case .integration: "puzzlepiece.extension"
        case .about: "info.circle"
        }
    }

    var groupTitle: String {
        switch self {
        case .engine, .integration, .about:
            "App"
        default:
            "Preferences"
        }
    }
}

struct SettingsView: View {
    @EnvironmentObject private var settings: AppSettings
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
        .themeBackground(settings.appTheme.theme.background)
    }

    private var settingsSidebar: some View {
        List(selection: $selection) {
            Section("Preferences") {
                ForEach(SettingsSection.orderedCases.filter { $0.groupTitle == "Preferences" }, id: \.self) { section in
                    Label(section.rawValue, systemImage: section.symbolName)
                        .tag(section)
                }
            }

            Section("App") {
                ForEach(SettingsSection.orderedCases.filter { $0.groupTitle == "App" }, id: \.self) { section in
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
        case .lookAndFeel:
            LookAndFeelSettingsPane()
        case .network:
            NetworkSettingsPane()
        case .locations:
            LocationsSettingsPane()
        case .siteLogins:
            SiteLoginsSettingsPane()
        case .power:
            PowerSettingsPane()
        case .engine:
            EngineSettingsPane()
        case .integration:
            IntegrationSettingsPane()
        case .about:
            AboutSettingsPane()
        }
    }
}

private struct LookAndFeelSettingsPane: View {
    @EnvironmentObject private var settings: AppSettings
    @AppStorage("showMenuBarIcon") private var showMenuBarIcon = true

    var body: some View {
        Form {
            Section("App Theme") {
                Picker("Theme", selection: $settings.appTheme) {
                    ForEach(AppTheme.allCases) { theme in
                        Text(theme.rawValue)
                            .tag(theme)
                    }
                }
                .pickerStyle(.radioGroup)
                
                Text("Select a color palette for the app's user interface.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            
            Section("Display") {
                Picker("Font Size", selection: $settings.appFontSize) {
                    ForEach(AppFontSize.allCases) { size in
                        Text(size.rawValue).tag(size)
                    }
                }
                
                Picker("List Row Density", selection: $settings.listRowDensity) {
                    ForEach(ListRowDensity.allCases) { density in
                        Text(density.rawValue).tag(density)
                    }
                }
            }
            
            Section("Menu Bar") {
                Toggle("Show menu bar icon", isOn: $showMenuBarIcon)
                
                Text("Provides quick access to downloads and queues from the macOS menu bar. Restart required if hiding.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
        }
        .formStyle(.grouped)
    }
}

private struct AboutSettingsPane: View {
    @StateObject private var updateChecker = AppUpdateChecker()
    @State private var availableUpdate: AvailableUpdate?

    private let developerProfileURL = URL(string: "https://github.com/nimbold")!
    private let projectURL = URL(string: "https://github.com/nimbold/Firelink")!
    private let aria2URL = URL(string: "https://aria2.github.io/")!
    private let licenseURL = URL(string: "https://github.com/nimbold/Firelink/blob/main/LICENSE")!

    private var appVersion: String {
        Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String ?? "0.1.0"
    }

    private var buildNumber: String {
        Bundle.main.infoDictionary?["CFBundleVersion"] as? String ?? "Development"
    }

    var body: some View {
        Form {
            Section {
                HStack(alignment: .center, spacing: 14) {
                    Image(nsImage: NSApp.applicationIconImage)
                        .resizable()
                        .frame(width: 56, height: 56)
                        .accessibilityHidden(true)

                    VStack(alignment: .leading, spacing: 4) {
                        Text("Firelink")
                            .font(.title2.weight(.semibold))
                        Text("Version \(appVersion)")
                            .foregroundStyle(.secondary)
                        Text("A native macOS download manager for fast, organized, segmented transfers.")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                }
                .padding(.vertical, 4)
            }

            Section("Updates") {
                LabeledContent("Status") {
                    Label(updateChecker.status.message, systemImage: updateStatusSymbol)
                        .foregroundStyle(updateStatusColor)
                }

                if let lastChecked = updateChecker.lastChecked {
                    LabeledContent("Last checked") {
                        Text(lastChecked, format: .dateTime.month().day().hour().minute())
                            .foregroundStyle(.secondary)
                    }
                }

                HStack {
                    Button {
                        Task {
                            await updateChecker.checkForUpdates(currentVersion: appVersion)
                        }
                    } label: {
                        Label("Check for Updates", systemImage: "arrow.clockwise")
                    }
                    .disabled(updateChecker.status == .checking)

                    Button {
                        openReleasesPage()
                    } label: {
                        Label("Open Releases", systemImage: "arrow.up.right.square")
                    }

                    Spacer()
                }

                if case .updateAvailable(_, let releaseURL) = updateChecker.status {
                    Button {
                        NSWorkspace.shared.open(releaseURL)
                    } label: {
                        Label("Download Latest Version", systemImage: "square.and.arrow.down")
                    }
                    .buttonStyle(.borderedProminent)
                }
            }

            Section("Developer") {
                LabeledContent("Created by") {
                    Text("NimBold")
                }

                LabeledContent("Source") {
                    Link("nimbold/Firelink", destination: projectURL)
                }
            }

            Section("Credits") {
                LabeledContent("Download engine") {
                    Link("aria2", destination: aria2URL)
                }

                Text("Firelink uses aria2c for segmented HTTP, HTTPS, FTP, and SFTP downloads.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }

            Section("Legal") {
                LabeledContent("License") {
                    Link("MIT License", destination: licenseURL)
                }

                Text("Copyright © 2026 NimBold. Firelink is released under the MIT License.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
        }
        .formStyle(.grouped)
        .onChange(of: updateChecker.status) { _, status in
            if case .updateAvailable(let latestVersion, let releaseURL) = status {
                availableUpdate = AvailableUpdate(version: latestVersion, url: releaseURL)
            }
        }
        .alert("Update Available", isPresented: Binding(
            get: { availableUpdate != nil },
            set: { isPresented in
                if !isPresented {
                    availableUpdate = nil
                }
            }
        )) {
            Button("Not Now", role: .cancel) {
                availableUpdate = nil
            }
            Button("Yes") {
                if let releaseURL = availableUpdate?.url {
                    NSWorkspace.shared.open(releaseURL)
                }
                availableUpdate = nil
            }
        } message: {
            Text("Firelink version \(availableUpdate?.version ?? "") is available. Do you want to open the download page?")
        }
    }

    private var updateStatusSymbol: String {
        switch updateChecker.status {
        case .idle:
            "sparkle.magnifyingglass"
        case .checking:
            "arrow.clockwise"
        case .upToDate:
            "checkmark.seal.fill"
        case .updateAvailable:
            "arrow.down.circle.fill"
        case .unavailable:
            "exclamationmark.triangle.fill"
        }
    }

    private var updateStatusColor: Color {
        switch updateChecker.status {
        case .idle, .checking:
            .secondary
        case .upToDate:
            .green
        case .updateAvailable:
            .accentColor
        case .unavailable:
            .orange
        }
    }

    private func openReleasesPage() {
        NSWorkspace.shared.open(updateChecker.releasesURL)
    }

    private struct AvailableUpdate: Equatable {
        var version: String
        var url: URL
    }
}

private struct EngineSettingsPane: View {
    private var executableURL: URL? {
        Aria2DownloadEngine.findExecutable()
    }

    private var version: String {
        Aria2DownloadEngine.versionString() ?? "Unavailable"
    }

    var body: some View {
        Form {
            Section {
                LabeledContent("Status") {
                    Label(
                        executableURL == nil ? "Missing" : "Ready",
                        systemImage: executableURL == nil ? "exclamationmark.triangle.fill" : "checkmark.seal.fill"
                    )
                    .foregroundStyle(executableURL == nil ? .orange : .green)
                }

                LabeledContent("Binary") {
                    Text(executableURL?.path ?? "Not found")
                        .font(.system(.body, design: .monospaced))
                        .textSelection(.enabled)
                }

                LabeledContent("Version") {
                    Text(version)
                        .font(.system(.body, design: .monospaced))
                        .textSelection(.enabled)
                }

                if executableURL == nil {
                    Text("Install aria2 with Homebrew or bundle aria2c inside the app resources.")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }
        }
        .formStyle(.grouped)
    }
}

private struct NetworkSettingsPane: View {
    @EnvironmentObject private var settings: AppSettings

    var body: some View {
        Form {
            Section {
                Picker("Proxy", selection: proxyBinding(\.mode)) {
                    ForEach(ProxyMode.allCases, id: \.self) { mode in
                        Text(mode.title)
                            .tag(mode)
                    }
                }
                .pickerStyle(.radioGroup)

                if settings.proxySettings.mode == .custom {
                    Picker("Proxy type", selection: proxyBinding(\.type)) {
                        ForEach(ProxyType.allCases, id: \.self) { type in
                            Text(type.title)
                                .tag(type)
                        }
                    }

                    Grid(alignment: .leading, horizontalSpacing: 10, verticalSpacing: 10) {
                        GridRow {
                            Text("IP or Host")
                            TextField("127.0.0.1", text: proxyBinding(\.host))
                                .textFieldStyle(.roundedBorder)
                                .font(.system(.body, design: .monospaced))
                        }

                        GridRow {
                            Text("Port")
                            TextField("8080", value: proxyBinding(\.port), format: .number)
                                .textFieldStyle(.roundedBorder)
                                .frame(width: 110)
                        }
                    }
                }

                Text(networkSummary)
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
        }
        .formStyle(.grouped)
    }

    private var networkSummary: String {
        switch settings.proxySettings.mode {
        case .none:
            "Downloads ignore configured proxies."
        case .system:
            "Downloads use the matching macOS system proxy when one is configured."
        case .custom:
            if let proxyURI = settings.proxySettings.customProxyURI {
                "Downloads use \(proxyURI)."
            } else {
                "Enter a proxy host and port to enable the custom proxy."
            }
        }
    }

    private func proxyBinding<Value>(_ keyPath: WritableKeyPath<ProxySettings, Value>) -> Binding<Value> {
        Binding {
            settings.proxySettings[keyPath: keyPath]
        } set: { newValue in
            var proxySettings = settings.proxySettings
            proxySettings[keyPath: keyPath] = newValue
            settings.proxySettings = proxySettings
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

            Section("Bandwidth") {
                Toggle("Limit total download speed", isOn: globalSpeedLimitEnabled)
                    .toggleStyle(.switch)

                Stepper(
                    "Global cap: \(settings.globalSpeedLimitKiBPerSecond) KiB/s",
                    value: globalSpeedLimitValue,
                    in: 1...10_485_760,
                    step: 128
                )
                .disabled(settings.globalSpeedLimitKiBPerSecond == 0)

                Text("Firelink splits this cap across the configured parallel download slots. Per-download limits can still be set lower in Add Downloads or Properties.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
        }
        .formStyle(.grouped)
    }

    private var globalSpeedLimitEnabled: Binding<Bool> {
        Binding {
            settings.globalSpeedLimitKiBPerSecond > 0
        } set: { isEnabled in
            settings.globalSpeedLimitKiBPerSecond = isEnabled ? max(settings.globalSpeedLimitKiBPerSecond, 1024) : 0
        }
    }

    private var globalSpeedLimitValue: Binding<Int> {
        Binding {
            max(settings.globalSpeedLimitKiBPerSecond, 1)
        } set: { newValue in
            settings.globalSpeedLimitKiBPerSecond = newValue
        }
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

private struct IntegrationSettingsPane: View {
    @State private var isInstalling = false
    @State private var firefoxApps: [URL] = []
    @State private var showAppPicker = false
    
    var body: some View {
        Form {
            Section {
                HStack(alignment: .center, spacing: 14) {
                    Image(systemName: "safari")
                        .resizable()
                        .frame(width: 48, height: 48)
                        .foregroundStyle(.orange)
                        .accessibilityHidden(true)

                    VStack(alignment: .leading, spacing: 4) {
                        Text("Firefox Extension")
                            .font(.title2.weight(.semibold))
                        Text("Capture downloads directly from your browser.")
                            .foregroundStyle(.secondary)
                    }
                }
                .padding(.vertical, 4)
            }
            
            Section {
                HStack {
                    Button {
                        handleInstallClick()
                    } label: {
                        Label("Install to Firefox", systemImage: "puzzlepiece.extension.fill")
                    }
                    .buttonStyle(.borderedProminent)
                    .disabled(isInstalling)
                    .confirmationDialog("Select Firefox Edition", isPresented: $showAppPicker, titleVisibility: .visible) {
                        ForEach(firefoxApps, id: \.self) { appURL in
                            Button(appURL.deletingPathExtension().lastPathComponent) {
                                installExtension(with: appURL)
                            }
                        }
                        Button("Cancel", role: .cancel) {}
                    } message: {
                        Text("Multiple Firefox installations were found. Which one would you like to install the extension to?")
                    }
                    
                    Button {
                        showExtensionInFinder()
                    } label: {
                        Label("Show in Finder", systemImage: "folder")
                    }
                }
                
                Text("Note: Mozilla strictly enforces add-on signing. Standard Firefox and Beta will block unsigned extensions. You must either use Firefox Developer Edition/Nightly (with xpinstall.signatures.required set to false) or load it temporarily via about:debugging.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            
            Section("Permissions & Privacy") {
                Text("The Firelink extension requests minimal permissions. It only reads your current tab when you explicitly click 'Download with Firelink' from the right-click menu, keeping your browsing history completely private.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
        }
        .formStyle(.grouped)
    }
    
    private func showExtensionInFinder() {
        guard let xpiURL = Bundle.main.url(forResource: "firelink", withExtension: "xpi") else { return }
        NSWorkspace.shared.activateFileViewerSelecting([xpiURL])
    }
    
    private func handleInstallClick() {
        let apps = findFirefoxApps()
        if apps.isEmpty {
            // Fallback to default macOS open behavior
            installExtension(with: nil)
        } else if apps.count == 1 {
            installExtension(with: apps[0])
        } else {
            firefoxApps = apps.sorted(by: { $0.lastPathComponent < $1.lastPathComponent })
            showAppPicker = true
        }
    }
    
    private func findFirefoxApps() -> [URL] {
        let directories = [
            URL(fileURLWithPath: "/Applications"),
            URL(fileURLWithPath: NSHomeDirectory() + "/Applications")
        ]
        
        var apps: [URL] = []
        for dir in directories {
            if let urls = try? FileManager.default.contentsOfDirectory(at: dir, includingPropertiesForKeys: nil) {
                for url in urls where url.pathExtension == "app" && url.lastPathComponent.lowercased().contains("firefox") {
                    apps.append(url)
                }
            }
        }
        return apps
    }
    
    private func installExtension(with appURL: URL?) {
        guard let xpiURL = Bundle.main.url(forResource: "firelink", withExtension: "xpi") else {
            print("Failed to find firelink.xpi in app bundle.")
            return
        }
        
        isInstalling = true
        Task {
            do {
                let process = Process()
                process.executableURL = URL(fileURLWithPath: "/usr/bin/open")
                if let appURL {
                    process.arguments = ["-a", appURL.path, xpiURL.path]
                } else {
                    process.arguments = [xpiURL.path] // Default open
                }
                try process.run()
                process.waitUntilExit()
            } catch {
                print("Failed to launch Firefox to install extension: \(error)")
            }
            isInstalling = false
        }
    }
}

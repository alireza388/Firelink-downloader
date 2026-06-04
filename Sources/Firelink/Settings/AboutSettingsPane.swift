import AppKit
import SwiftUI

struct AboutSettingsPane: View {
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

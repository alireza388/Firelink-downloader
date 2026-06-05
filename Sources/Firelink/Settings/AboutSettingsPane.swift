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
                HStack(alignment: .center, spacing: 16) {
                    Image(nsImage: NSApp.applicationIconImage)
                        .resizable()
                        .frame(width: 64, height: 64)
                        .accessibilityHidden(true)

                    VStack(alignment: .leading, spacing: 4) {
                        Text("Firelink")
                            .font(.title2.weight(.bold))
                        Text("Version \(appVersion)")
                            .foregroundStyle(.secondary)
                        Text("A native macOS download manager for fast, organized, segmented transfers.")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                }
                .padding(.vertical, 8)
            }

            Section("Updates") {
                VStack(alignment: .leading, spacing: 12) {
                    HStack {
                        Label(updateChecker.status.message, systemImage: updateStatusSymbol)
                            .foregroundStyle(updateStatusColor)
                        
                        Spacer()
                        
                        if let lastChecked = updateChecker.lastChecked {
                            Text("Last checked: \(lastChecked, format: .dateTime.month().day().hour().minute())")
                                .font(.caption)
                                .foregroundStyle(.secondary)
                        }
                    }

                    HStack(spacing: 12) {
                        Button {
                            Task {
                                await updateChecker.checkForUpdates(currentVersion: appVersion)
                            }
                        } label: {
                            Label("Check for Updates", systemImage: "arrow.clockwise")
                        }
                        .disabled(updateChecker.status == .checking)

                        if case .updateAvailable(_, let releaseURL) = updateChecker.status {
                            Button {
                                NSWorkspace.shared.open(releaseURL)
                            } label: {
                                Label("Download Latest Version", systemImage: "square.and.arrow.down")
                            }
                            .buttonStyle(.borderedProminent)
                        } else {
                            Button {
                                openReleasesPage()
                            } label: {
                                Label("Open Releases", systemImage: "arrow.up.right.square")
                            }
                        }
                    }
                }
                .padding(.vertical, 4)
            }

            Section {
                VStack(alignment: .leading, spacing: 10) {
                    HStack {
                        Text("Created by NimBold")
                        Spacer()
                        Link(destination: projectURL) {
                            HStack(spacing: 4) {
                                if let imgPath = Bundle.main.path(forResource: "GitHubTemplate", ofType: "png"),
                                   let nsImage = NSImage(contentsOfFile: imgPath) {
                                    Image(nsImage: nsImage)
                                        .resizable()
                                        .renderingMode(.template)
                                        .frame(width: 14, height: 14)
                                        .foregroundStyle(.secondary)
                                }
                                Text("Source Code")
                            }
                        }
                        .buttonStyle(.plain)
                    }

                    HStack {
                        Text("Powered by")
                        Link("aria2", destination: aria2URL)
                        Spacer()
                        Link("MIT License", destination: licenseURL)
                    }

                    Text("Copyright © 2026 NimBold. All rights reserved.")
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                }
                .font(.caption)
                .padding(.vertical, 4)
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

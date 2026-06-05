import AppKit
import SwiftUI

struct IntegrationSettingsPane: View {
    @EnvironmentObject private var controller: DownloadController
    @State private var copiedExtensionURL: URL?
    @State private var installMessage = ""

    var body: some View {
        Form {
            Section {
                HStack(alignment: .center, spacing: 14) {
                    Image(systemName: "puzzlepiece.extension")
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
            
            Section("Installation") {
                HStack(spacing: 10) {
                    Button {
                        copyExtensionToDownloads()
                    } label: {
                        Label("Copy to Downloads", systemImage: "folder.badge.plus")
                    }
                    
                    Button {
                        showCopiedExtensionInFinder()
                    } label: {
                        Label("Show Copied Folder", systemImage: "folder.fill")
                    }
                    .disabled(copiedExtensionURL == nil)

                    Button {
                        openFirefoxDebugging()
                    } label: {
                        Label("Open Firefox Debugging", systemImage: "link")
                    }
                }
                .buttonStyle(.borderedProminent)

                if !installMessage.isEmpty {
                    Text(installMessage)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                
                Text("Firelink Companion has been submitted to Mozilla and is awaiting review. Until it is approved, load the extension manually:\n1. Click 'Copy to Downloads'.\n2. Click 'Open Firefox Debugging'.\n3. Click 'This Firefox' on the left sidebar.\n4. Click 'Load Temporary Add-on' and select manifest.json inside Downloads/Firelink Firefox Extension.\n\nKeep the copied folder while Firefox is running. Temporary add-ons are removed when Firefox restarts, so you can delete the folder after restart or after installing the approved add-on.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }

            Section("Diagnostics") {
                LabeledContent("Local receiver") {
                    if let port = controller.extensionServerPort {
                        Label("Listening on 127.0.0.1:\(port)", systemImage: "checkmark.seal.fill")
                            .foregroundStyle(.green)
                    } else {
                        Label("Not listening", systemImage: "exclamationmark.triangle.fill")
                            .foregroundStyle(.orange)
                    }
                }

                LabeledContent("Copied folder") {
                    if FileManager.default.fileExists(atPath: downloadsExtensionURL.appendingPathComponent("manifest.json").path) {
                        Label("Ready", systemImage: "checkmark.seal.fill")
                            .foregroundStyle(.green)
                    } else {
                        Label("Not copied", systemImage: "folder.badge.questionmark")
                            .foregroundStyle(.secondary)
                    }
                }
            }
            
            Section("Permissions & Privacy") {
                Text("The Firelink extension uses download, context menu, storage, active tab, scripting, and local Firelink endpoint permissions. It reads the active tab URL for per-site settings and explicit right-click actions, and forwards download URLs only when you use a Firelink action or enable global capture.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
        }
        .formStyle(.grouped)
        .onAppear {
            if FileManager.default.fileExists(atPath: downloadsExtensionURL.path) {
                copiedExtensionURL = downloadsExtensionURL
            }
        }
    }
    
    private var downloadsExtensionURL: URL {
        let downloads = FileManager.default.urls(for: .downloadsDirectory, in: .userDomainMask).first
            ?? URL(fileURLWithPath: NSHomeDirectory()).appendingPathComponent("Downloads")
        return downloads.appendingPathComponent("Firelink Firefox Extension", isDirectory: true)
    }

    private func copyExtensionToDownloads() {
        guard let sourceURL = bundledFirefoxExtensionURL() else {
            installMessage = "The bundled Firefox extension folder was not found."
            return
        }

        let destinationURL = downloadsExtensionURL
        do {
            if FileManager.default.fileExists(atPath: destinationURL.path) {
                try FileManager.default.removeItem(at: destinationURL)
            }

            try copyFirefoxExtension(from: sourceURL, to: destinationURL)
            copiedExtensionURL = destinationURL
            installMessage = "Copied to \(destinationURL.path). Select manifest.json from this folder in Firefox."
            showCopiedExtensionInFinder()
        } catch {
            installMessage = "Could not copy the extension to Downloads: \(error.localizedDescription)"
        }
    }

    private func bundledFirefoxExtensionURL() -> URL? {
        if let bundled = Bundle.main.url(forResource: "FirefoxExtension", withExtension: nil) {
            return bundled
        }

        let sourceFile = URL(fileURLWithPath: #filePath)
        let projectRoot = sourceFile
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
        let sourceTreeExtension = projectRoot.appendingPathComponent("Extensions/Firefox", isDirectory: true)
        return FileManager.default.fileExists(atPath: sourceTreeExtension.appendingPathComponent("manifest.json").path)
            ? sourceTreeExtension
            : nil
    }

    private func copyFirefoxExtension(from sourceURL: URL, to destinationURL: URL) throws {
        let fileManager = FileManager.default
        try fileManager.createDirectory(at: destinationURL, withIntermediateDirectories: true)

        for component in ["background.js", "content.js", "manifest.json", "icons", "popup"] {
            try fileManager.copyItem(
                at: sourceURL.appendingPathComponent(component),
                to: destinationURL.appendingPathComponent(component)
            )
        }
    }

    private func showCopiedExtensionInFinder() {
        let folderURL = copiedExtensionURL ?? downloadsExtensionURL
        let manifestURL = folderURL.appendingPathComponent("manifest.json")
        if FileManager.default.fileExists(atPath: manifestURL.path) {
            NSWorkspace.shared.activateFileViewerSelecting([manifestURL])
        } else if FileManager.default.fileExists(atPath: folderURL.path) {
            NSWorkspace.shared.activateFileViewerSelecting([folderURL])
        }
    }

    private func openFirefoxDebugging() {
        let bundleIDs = [
            "org.mozilla.firefoxdeveloperedition",
            "org.mozilla.firefox",
            "org.mozilla.nightly"
        ]
        
        let workspace = NSWorkspace.shared
        for id in bundleIDs {
            if let appURL = workspace.urlForApplication(withBundleIdentifier: id) {
                let process = Process()
                process.executableURL = URL(fileURLWithPath: "/usr/bin/open")
                process.arguments = ["-a", appURL.path, "about:debugging"]
                try? process.run()
                return
            }
        }
        
        // Fallback
        if let fallbackURL = URL(string: "about:debugging") {
            workspace.open(fallbackURL)
        }
    }
}

import SwiftUI
import AppKit

struct EngineSettingsPane: View {
    @State private var version = "Checking..."

    private var executableURL: URL? {
        Aria2DownloadEngine.findExecutable()
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
        .task {
            version = await Aria2DownloadEngine.versionString() ?? "Unavailable"
        }
    }
}

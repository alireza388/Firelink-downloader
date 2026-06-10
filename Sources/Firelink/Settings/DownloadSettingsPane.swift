import SwiftUI

struct DownloadSettingsPane: View {
    @EnvironmentObject private var settings: AppSettings

    var body: some View {
        Form {
            Section("Connections") {
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
                LabeledContent("Global speed limit") {
                    HStack {
                        TextField("0", value: $settings.globalSpeedLimitKiBPerSecond, format: .number)
                            .textFieldStyle(.roundedBorder)
                            .multilineTextAlignment(.trailing)
                            .frame(width: 80)
                        Text("KiB/s")
                            .foregroundStyle(.secondary)
                    }
                }
                Text("Set to 0 for unlimited speed. This limit is divided across currently active downloads.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            
            Section("Recovery") {
                Stepper(
                    "Automatic retries: \(settings.maxAutomaticRetries)",
                    value: $settings.maxAutomaticRetries,
                    in: 0...10
                )
                Text("Number of times to retry a download automatically if the connection fails.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            
            Section("Notifications") {
                Toggle("Show notification when download completes", isOn: $settings.showNotifications)
                Toggle("Play sound when download completes", isOn: $settings.playCompletionSound)
                    .disabled(!settings.showNotifications)
            }
        }
        .formStyle(.grouped)
    }
}

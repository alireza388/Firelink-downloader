import SwiftUI

struct PowerSettingsPane: View {
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

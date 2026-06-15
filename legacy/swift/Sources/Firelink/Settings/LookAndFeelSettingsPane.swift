import SwiftUI

struct LookAndFeelSettingsPane: View {
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

            Section("macOS Integration") {
                Toggle("Show badge on Dock icon", isOn: $settings.showDockBadge)
                Text("Displays the number of active downloads on the Firelink Dock icon.")
                    .font(.caption)
                    .foregroundStyle(.secondary)

                Toggle("Show menu bar icon", isOn: $showMenuBarIcon)
                Text("Provides quick access to downloads and queues from the macOS menu bar.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
        }
        .formStyle(.grouped)
    }
}

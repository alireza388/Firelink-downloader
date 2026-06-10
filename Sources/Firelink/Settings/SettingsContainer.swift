import AppKit
import SwiftUI

enum SettingsSidebarFilter: String, CaseIterable, Hashable {
    case downloads = "Downloads"
    case lookAndFeel = "Look and feel"
    case network = "Network"
    case locations = "Locations"
    case siteLogins = "Site Logins"
    case power = "Power"
    case engine = "Engine"
    case integration = "Integrations"
    case about = "About"

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
}

struct SettingsPaneContainer: View {
    @AppStorage("lastSettingsTab") private var activeTab: SettingsSidebarFilter = .downloads

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 4) {
                ForEach(SettingsSidebarFilter.allCases, id: \.self) { filter in
                    Button {
                        activeTab = filter
                    } label: {
                        VStack(spacing: 4) {
                            Image(systemName: filter.symbolName)
                                .font(.system(size: 16))
                            Text(filter.rawValue)
                                .font(.system(size: 11, weight: .medium))
                                .lineLimit(1)
                                .minimumScaleFactor(0.8)
                        }
                        .frame(maxWidth: .infinity)
                        .padding(.vertical, 8)
                        .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                    .background(activeTab == filter ? Color.accentColor : Color.clear)
                    .foregroundStyle(activeTab == filter ? Color.white : Color.primary)
                    .clipShape(RoundedRectangle(cornerRadius: 8))
                }
            }
            .padding(.horizontal, 32)
            .padding(.vertical, 16)

            Divider()

            ScrollView {
                VStack(alignment: .leading, spacing: 0) {
                    Text(activeTab.rawValue)
                        .font(.largeTitle.weight(.semibold))
                        .padding(.bottom, 24)

                    selectedPane
                        .frame(maxWidth: 720, alignment: .leading)
                }
                .padding(32)
                .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    @ViewBuilder
    private var selectedPane: some View {
        switch activeTab {
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

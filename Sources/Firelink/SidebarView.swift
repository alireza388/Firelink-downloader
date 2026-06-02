import SwiftUI

enum DownloadSidebarFilter: Hashable {
    case all
    case queued
    case active
    case completed
    case failed
    case category(DownloadCategory)

    var title: String {
        switch self {
        case .all: "All Downloads"
        case .queued: "Queue"
        case .active: "Active"
        case .completed: "Completed"
        case .failed: "Failed"
        case .category(let category): category.rawValue
        }
    }
}

enum SidebarSelection: Hashable {
    case downloads(DownloadSidebarFilter)
    case settings
}

struct SidebarView: View {
    @EnvironmentObject private var controller: DownloadController
    @Binding var selection: SidebarSelection

    var body: some View {
        List(selection: $selection) {
            Section("Library") {
                Label("All", systemImage: "tray.full")
                    .badge(controller.downloads.count)
                    .tag(SidebarSelection.downloads(.all))
                Label("Queue", systemImage: "list.bullet")
                    .badge(controller.queuedCount)
                    .tag(SidebarSelection.downloads(.queued))
                Label("Active", systemImage: "bolt.fill")
                    .badge(controller.activeCount)
                    .tag(SidebarSelection.downloads(.active))
                Label("Completed", systemImage: "checkmark.circle")
                    .badge(controller.completedCount)
                    .tag(SidebarSelection.downloads(.completed))
                Label("Failed", systemImage: "exclamationmark.triangle")
                    .badge(controller.failedCount)
                    .tag(SidebarSelection.downloads(.failed))
            }

            Section("Folders") {
                ForEach(DownloadCategory.allCases, id: \.self) { category in
                    Label(category.rawValue, systemImage: category.symbolName)
                        .badge(controller.downloads.filter { $0.category == category }.count)
                        .tag(SidebarSelection.downloads(.category(category)))
                }
            }
        }
        .listStyle(.sidebar)
        .safeAreaInset(edge: .bottom) {
            VStack(spacing: 8) {
                Divider()
                Button {
                    selection = .settings
                } label: {
                    Label("Settings", systemImage: "gearshape")
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .padding(.horizontal, 10)
                        .padding(.vertical, 7)
                        .contentShape(RoundedRectangle(cornerRadius: 6))
                }
                .buttonStyle(.plain)
                .background {
                    if selection == .settings {
                        RoundedRectangle(cornerRadius: 6)
                            .fill(Color.accentColor.opacity(0.14))
                    }
                }
                .padding(.horizontal, 8)
                .padding(.bottom, 8)
            }
            .background(.bar)
        }
    }
}

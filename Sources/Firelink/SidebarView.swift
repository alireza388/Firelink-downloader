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

struct SidebarView: View {
    @EnvironmentObject private var controller: DownloadController
    @Binding var selection: DownloadSidebarFilter

    var body: some View {
        List(selection: $selection) {
            Section("Library") {
                Label("All", systemImage: "tray.full")
                    .badge(controller.downloads.count)
                    .tag(DownloadSidebarFilter.all)
                Label("Queue", systemImage: "list.bullet")
                    .badge(controller.queuedCount)
                    .tag(DownloadSidebarFilter.queued)
                Label("Active", systemImage: "bolt.fill")
                    .badge(controller.activeCount)
                    .tag(DownloadSidebarFilter.active)
                Label("Completed", systemImage: "checkmark.circle")
                    .badge(controller.completedCount)
                    .tag(DownloadSidebarFilter.completed)
                Label("Failed", systemImage: "exclamationmark.triangle")
                    .badge(controller.failedCount)
                    .tag(DownloadSidebarFilter.failed)
            }

            Section("Folders") {
                ForEach(DownloadCategory.allCases, id: \.self) { category in
                    Label(category.rawValue, systemImage: category.symbolName)
                        .badge(controller.downloads.filter { $0.category == category }.count)
                        .tag(DownloadSidebarFilter.category(category))
                }
            }

            Section("Engine") {
                HStack {
                    Image(systemName: controller.hasAria2 ? "checkmark.seal.fill" : "exclamationmark.triangle.fill")
                        .foregroundStyle(controller.hasAria2 ? .green : .orange)
                    Text(controller.hasAria2 ? "aria2c ready" : "aria2c missing")
                }
            }
        }
        .listStyle(.sidebar)
    }
}

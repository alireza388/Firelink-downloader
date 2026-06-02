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
    case queue(UUID)
    case settings
}

struct SidebarView: View {
    @EnvironmentObject private var controller: DownloadController
    @Binding var selection: SidebarSelection
    @State private var queueBeingRenamed: DownloadQueue?
    @State private var queueName = ""

    var body: some View {
        List(selection: $selection) {
            Section("Library") {
                Label("All", systemImage: "tray.full")
                    .badge(controller.downloads.count)
                    .tag(SidebarSelection.downloads(.all))
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

            Section("Queues") {
                ForEach(controller.queues) { queue in
                    Label(queue.name, systemImage: queue.isMain ? "list.bullet.rectangle" : "list.bullet")
                        .badge(controller.queueCount(for: queue.id))
                        .tag(SidebarSelection.queue(queue.id))
                        .contextMenu {
                            if !queue.isMain {
                                Button("Rename") {
                                    queueBeingRenamed = queue
                                    queueName = queue.name
                                }
                            }
                        }
                }

                Button {
                    let queue = controller.addQueue()
                    selection = .queue(queue.id)
                } label: {
                    Label("Add new queue", systemImage: "plus")
                }
                .buttonStyle(.plain)
            }
        }
        .listStyle(.sidebar)
        .alert("Rename Queue", isPresented: Binding(
            get: { queueBeingRenamed != nil },
            set: { isPresented in
                if !isPresented {
                    queueBeingRenamed = nil
                }
            }
        )) {
            TextField("Queue name", text: $queueName)
            Button("Rename") {
                if let queue = queueBeingRenamed {
                    controller.renameQueue(id: queue.id, name: queueName)
                }
                queueBeingRenamed = nil
            }
            Button("Cancel", role: .cancel) {
                queueBeingRenamed = nil
            }
        }
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

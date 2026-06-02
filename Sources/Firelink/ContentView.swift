import AppKit
import SwiftUI

struct ContentView: View {
    @EnvironmentObject private var controller: DownloadController
    @Environment(\.openWindow) private var openWindow
    @State private var selection: Set<DownloadItem.ID> = []
    @State private var sidebarSelection: SidebarSelection = .downloads(.all)
    @State private var showDeleteConfirmation = false

    var body: some View {
        NavigationSplitView {
            SidebarView(selection: $sidebarSelection)
                .navigationSplitViewColumnWidth(min: 190, ideal: 220, max: 260)
        } detail: {
            detailView
        }
    }

    @ViewBuilder
    private var detailView: some View {
        switch sidebarSelection {
        case .downloads(let filter):
            downloadsView(filter: filter)
        case .settings:
            SettingsView()
        }
    }

    private func downloadsView(filter: DownloadSidebarFilter) -> some View {
        VStack(spacing: 0) {
            DownloadTable(
                items: filteredDownloads(for: filter),
                selection: $selection,
                title: filter.title
            )
            Divider()
            StatusBar()
        }
        .toolbar {
            ToolbarItem(placement: .primaryAction) {
                Button {
                    openWindow(id: "add-downloads")
                } label: {
                    Label("Add", systemImage: "plus")
                }
            }

            ToolbarItem {
                Button {
                    controller.startQueue()
                } label: {
                    Label("Start Queue", systemImage: "play.fill")
                }
            }

            ToolbarItemGroup {
                if !selectedItems.isEmpty {
                    if selectedItems.contains(where: { $0.status == .downloading }) {
                        Button {
                            for item in selectedItems where item.status == .downloading {
                                controller.pause(item)
                            }
                        } label: {
                            Label("Stop", systemImage: "stop.fill")
                        }
                    }

                    if selectedItems.contains(where: { $0.status == .paused || $0.status == .failed || $0.status == .canceled }) {
                        Button {
                            for item in selectedItems where item.status == .paused || item.status == .failed || item.status == .canceled {
                                controller.resume(item)
                            }
                        } label: {
                            Label("Start", systemImage: "play.fill")
                        }
                    }

                    Button(role: .destructive) {
                        showDeleteConfirmation = true
                    } label: {
                        Label("Delete", systemImage: "trash")
                    }
                }
            }
        }
        .background {
            Button("") {
                if !selection.isEmpty {
                    showDeleteConfirmation = true
                }
            }
            .keyboardShortcut(.delete, modifiers: [])
            .opacity(0)

            Button("") {
                handlePaste()
            }
            .keyboardShortcut("v", modifiers: .command)
            .opacity(0)

            Button("") {
                selectAll(filter: filter)
            }
            .keyboardShortcut("a", modifiers: .command)
            .opacity(0)
        }
        .confirmationDialog(
            "Delete \(selection.count) Download\(selection.count == 1 ? "" : "s")",
            isPresented: $showDeleteConfirmation
        ) {
            Button("Remove from List") {
                deleteSelected(deleteFiles: false)
            }
            Button("Move Files and Cache to Trash", role: .destructive) {
                deleteSelected(deleteFiles: true)
            }
            Button("Cancel", role: .cancel) {}
        } message: {
            Text("Are you sure you want to delete the selected downloads?")
        }
    }

    private var selectedItems: [DownloadItem] {
        controller.downloads.filter { selection.contains($0.id) }
    }

    private func deleteSelected(deleteFiles: Bool) {
        for item in selectedItems {
            controller.delete(item, deleteFiles: deleteFiles)
        }
        selection.removeAll()
    }
    
    private func handlePaste() {
        guard let text = NSPasteboard.general.string(forType: .string), !text.isEmpty else { return }
        controller.pendingPasteboardText = text
        openWindow(id: "add-downloads")
    }

    private func selectAll(filter: DownloadSidebarFilter) {
        selection = Set(filteredDownloads(for: filter).map { $0.id })
    }

    private func filteredDownloads(for filter: DownloadSidebarFilter) -> [DownloadItem] {
        switch filter {
        case .all:
            controller.downloads
        case .queued:
            controller.downloads.filter { $0.status == .queued }
        case .active:
            controller.downloads.filter { $0.status == .downloading }
        case .completed:
            controller.downloads.filter { $0.status == .completed }
        case .failed:
            controller.downloads.filter { $0.status == .failed }
        case .category(let category):
            controller.downloads.filter { $0.category == category }
        }
    }
}

import AppKit
import SwiftUI

struct ContentView: View {
    @EnvironmentObject private var controller: DownloadController
    @Environment(\.openWindow) private var openWindow
    @State private var selection: DownloadItem.ID?
    @State private var sidebarFilter: DownloadSidebarFilter = .all

    var body: some View {
        NavigationSplitView {
            SidebarView(selection: $sidebarFilter)
                .navigationSplitViewColumnWidth(min: 190, ideal: 220, max: 260)
        } detail: {
            VStack(spacing: 0) {
                DownloadTable(
                    items: filteredDownloads,
                    selection: $selection,
                    title: sidebarFilter.title
                )
                Divider()
                StatusBar()
            }
            .toolbar {
                ToolbarItemGroup {
                    Button {
                        openWindow(id: "add-downloads")
                    } label: {
                        Label("Add", systemImage: "plus")
                    }
                    .buttonStyle(.borderedProminent)

                    Button {
                        controller.startQueue()
                    } label: {
                        Label("Start Queue", systemImage: "play.fill")
                    }

                    SettingsLink {
                        Label("Settings", systemImage: "gearshape")
                    }

                    if let selectedItem {
                        if selectedItem.status == .downloading {
                            Button {
                                controller.pause(selectedItem)
                            } label: {
                                Label("Stop", systemImage: "stop.fill")
                            }
                        } else if selectedItem.status == .paused || selectedItem.status == .failed || selectedItem.status == .canceled {
                            Button {
                                controller.resume(selectedItem)
                            } label: {
                                Label("Resume", systemImage: "arrow.clockwise")
                            }
                        }

                        Button(role: .destructive) {
                            controller.cancel(selectedItem)
                        } label: {
                            Label("Cancel", systemImage: "xmark")
                        }
                    }
                }
            }
        }
    }

    private var selectedItem: DownloadItem? {
        guard let selection else { return nil }
        return controller.downloads.first(where: { $0.id == selection })
    }

    private var filteredDownloads: [DownloadItem] {
        switch sidebarFilter {
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

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

private struct SidebarView: View {
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

enum DownloadColumn: String, CaseIterable, Identifiable {
    case fileName = "File name"
    case size = "Size"
    case progress = "Progress"
    case status = "Status"
    case lastTry = "Last try date"
    case dateAdded = "Date added"
    case category = "Category"
    case connections = "Connections"
    case liveConnections = "Live conn."
    case speed = "Speed"
    case eta = "ETA"
    case destination = "Save location"
    case url = "URL"
    case message = "Message"

    var id: String { rawValue }

    var width: CGFloat {
        switch self {
        case .fileName: 340
        case .size: 100
        case .status: 105
        case .progress: 115
        case .lastTry, .dateAdded: 155
        case .category: 105
        case .connections, .liveConnections: 95
        case .speed, .eta: 90
        case .destination: 240
        case .url: 280
        case .message: 220
        }
    }
}

enum SortDirection {
    case ascending
    case descending

    mutating func toggle() {
        self = self == .ascending ? .descending : .ascending
    }
}

private struct DownloadTable: View {
    @EnvironmentObject private var controller: DownloadController
    @EnvironmentObject private var settings: AppSettings
    @Environment(\.openWindow) private var openWindow
    let items: [DownloadItem]
    @Binding var selection: DownloadItem.ID?
    let title: String

    @State private var visibleColumns: Set<DownloadColumn> = [
        .fileName, .size, .progress, .eta, .lastTry, .dateAdded
    ]
    @State private var columnWidths = Dictionary(
        uniqueKeysWithValues: DownloadColumn.allCases.map { ($0, $0.width) }
    )
    @State private var sortColumn: DownloadColumn = .dateAdded
    @State private var sortDirection: SortDirection = .descending
    @State private var pendingDeleteItem: DownloadItem?
    @State private var resizeBaseWidths: [DownloadColumn: CGFloat] = [:]

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                Text(title)
                    .font(.headline)
                Text("\(items.count)")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .padding(.horizontal, 7)
                    .padding(.vertical, 3)
                    .background(.quaternary.opacity(0.5))
                    .clipShape(RoundedRectangle(cornerRadius: 5))
                Spacer()
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 8)
            GeometryReader { proxy in
                let tableWidth = max(totalWidth, proxy.size.width)
                let trailingWidth = max(0, tableWidth - totalWidth)

                ScrollView(.horizontal) {
                    VStack(spacing: 0) {
                        tableHeader(trailingWidth: trailingWidth)
                        Divider()
                        ScrollView(.vertical) {
                            LazyVStack(spacing: 0) {
                                ForEach(sortedItems) { item in
                                    tableRow(for: item, tableWidth: tableWidth, trailingWidth: trailingWidth)
                                    Divider()
                                }
                            }
                            .frame(width: tableWidth, alignment: .topLeading)
                            .frame(maxHeight: .infinity, alignment: .topLeading)
                        }
                        .defaultScrollAnchor(.topLeading)
                    }
                    .frame(width: tableWidth, height: proxy.size.height, alignment: .topLeading)
                }
                .overlay {
                    if items.isEmpty {
                        ContentUnavailableView(
                            "No Downloads",
                            systemImage: "arrow.down.circle",
                            description: Text("Use Add to paste one or more links.")
                        )
                    }
                }
            }
        }
        .confirmationDialog(
            "Delete Download",
            isPresented: Binding(
                get: { pendingDeleteItem != nil },
                set: { isPresented in
                    if !isPresented {
                        pendingDeleteItem = nil
                    }
                }
            ),
            presenting: pendingDeleteItem
        ) { item in
            Button("Remove from List") {
                controller.delete(item, deleteFiles: false)
                pendingDeleteItem = nil
            }

            Button("Move File and Cache to Trash", role: .destructive) {
                controller.delete(item, deleteFiles: true)
                pendingDeleteItem = nil
            }

            Button("Cancel", role: .cancel) {
                pendingDeleteItem = nil
            }
        } message: { item in
            if item.status == .completed {
                Text("Remove this download from Firelink, or also move the downloaded file to Trash.")
            } else {
                Text("Remove this unfinished download from Firelink. Partial cache files are removed automatically; moving to Trash also sends any partial file there.")
            }
        }
    }

    private func tableRow(for item: DownloadItem, tableWidth: CGFloat, trailingWidth: CGFloat) -> some View {
        DownloadRow(
            item: item,
            visibleColumns: orderedVisibleColumns,
            columnWidth: { width(for: $0) },
            trailingWidth: trailingWidth
        )
        .id(item.id)
        .frame(width: tableWidth, alignment: .leading)
        .background(selection == item.id ? Color.accentColor.opacity(0.12) : Color.clear)
        .contentShape(Rectangle())
        .onTapGesture {
            selection = item.id
        }
        .contextMenu {
            rowContextMenu(for: item)
        }
    }

    private func tableHeader(trailingWidth: CGFloat) -> some View {
        HStack(spacing: 0) {
            ForEach(orderedVisibleColumns) { column in
                ZStack(alignment: .trailing) {
                    Button {
                        if sortColumn == column {
                            sortDirection.toggle()
                        } else {
                            sortColumn = column
                            sortDirection = .ascending
                        }
                    } label: {
                        HStack(spacing: 4) {
                            Spacer(minLength: 0)
                            Text(column.rawValue)
                                .font(.caption.weight(.semibold))
                                .lineLimit(1)
                                .truncationMode(.tail)
                                .multilineTextAlignment(.center)
                                .layoutPriority(1)
                            if sortColumn == column {
                                Image(systemName: sortDirection == .ascending ? "chevron.up" : "chevron.down")
                                    .font(.caption2)
                            }
                            Spacer(minLength: 0)
                        }
                        .padding(.horizontal, 8)
                        .frame(width: width(for: column), height: 34, alignment: .center)
                        .clipped()
                    }
                    .buttonStyle(.plain)

                    Rectangle()
                        .fill(.secondary.opacity(0.18))
                        .frame(width: 1, height: 20)
                        .frame(width: 8, height: 34)
                        .contentShape(Rectangle())
                        .onHover { isHovering in
                            if isHovering {
                                NSCursor.resizeLeftRight.push()
                            } else {
                                NSCursor.pop()
                            }
                        }
                        .gesture(
                            DragGesture(minimumDistance: 1)
                                .onChanged { value in
                                    let baseWidth = resizeBaseWidths[column] ?? width(for: column)
                                    resizeBaseWidths[column] = baseWidth
                                    columnWidths[column] = max(70, baseWidth + value.translation.width)
                                }
                                .onEnded { _ in
                                    resizeBaseWidths[column] = nil
                                }
                        )
                }
                .frame(width: width(for: column), height: 34)
                .clipped()
            }

            if trailingWidth > 0 {
                Color.clear
                    .frame(width: trailingWidth, height: 34)
            }
        }
        .background(.bar)
        .contextMenu {
            Section("Columns") {
                ForEach(DownloadColumn.allCases) { column in
                    Toggle(column.rawValue, isOn: Binding(
                        get: { visibleColumns.contains(column) },
                        set: { isVisible in
                            if isVisible {
                                visibleColumns.insert(column)
                            } else if visibleColumns.count > 1 {
                                visibleColumns.remove(column)
                            }
                        }
                    ))
                }
            }
            Section("Sort By") {
                ForEach(DownloadColumn.allCases) { column in
                    Button(column.rawValue) {
                        sortColumn = column
                    }
                }
            }
        }
    }

    @ViewBuilder
    private func rowContextMenu(for item: DownloadItem) -> some View {
        Button {
            openWindow(value: item.id)
        } label: {
            Label("Properties", systemImage: "info.circle")
        }

        Button {
            showInFinder(item)
        } label: {
            Label("Show in Finder", systemImage: "finder")
        }

        Divider()

        Button {
            controller.resume(item)
        } label: {
            Label("Resume", systemImage: "arrow.clockwise")
        }
        .disabled(item.status == .downloading)

        Button {
            controller.pause(item)
        } label: {
            Label("Stop", systemImage: "stop.fill")
        }
        .disabled(item.status != .downloading)

        Button {
            controller.queue(item)
        } label: {
            Label("Add to Queue", systemImage: "list.bullet")
        }
        .disabled(item.status == .downloading || item.status == .queued)

        Divider()

        Button(role: .destructive) {
            pendingDeleteItem = item
        } label: {
            Label("Delete", systemImage: "trash")
        }
    }

    private var orderedVisibleColumns: [DownloadColumn] {
        DownloadColumn.allCases.filter { visibleColumns.contains($0) }
    }

    private var totalWidth: CGFloat {
        orderedVisibleColumns.map { width(for: $0) }.reduce(0, +)
    }

    private func width(for column: DownloadColumn) -> CGFloat {
        max(70, columnWidths[column] ?? column.width)
    }

    private var sortedItems: [DownloadItem] {
        items.sorted { lhs, rhs in
            let result = compare(lhs, rhs, by: sortColumn)
            if result == .orderedSame {
                return lhs.id.uuidString < rhs.id.uuidString
            }
            return sortDirection == .ascending ? result == .orderedAscending : result == .orderedDescending
        }
    }

    private func compare(_ lhs: DownloadItem, _ rhs: DownloadItem, by column: DownloadColumn) -> ComparisonResult {
        switch column {
        case .fileName: lhs.fileName.localizedCaseInsensitiveCompare(rhs.fileName)
        case .size: compare(lhs.sizeBytes ?? -1, rhs.sizeBytes ?? -1)
        case .status: compare(lhs.status.rawValue, rhs.status.rawValue)
        case .progress: compare(lhs.progress, rhs.progress)
        case .lastTry: compare(lhs.lastTryAt ?? .distantPast, rhs.lastTryAt ?? .distantPast)
        case .dateAdded: compare(lhs.createdAt, rhs.createdAt)
        case .category: compare(lhs.category.rawValue, rhs.category.rawValue)
        case .connections: compare(lhs.connectionsPerServer, rhs.connectionsPerServer)
        case .liveConnections: compare(lhs.connectionCount, rhs.connectionCount)
        case .speed: compare(lhs.speedText, rhs.speedText)
        case .eta: compare(lhs.etaText, rhs.etaText)
        case .destination: compare(lhs.destinationPath, rhs.destinationPath)
        case .url: compare(lhs.url.absoluteString, rhs.url.absoluteString)
        case .message: compare(lhs.message, rhs.message)
        }
    }

    private func compare<T: Comparable>(_ lhs: T, _ rhs: T) -> ComparisonResult {
        if lhs < rhs { return .orderedAscending }
        if lhs > rhs { return .orderedDescending }
        return .orderedSame
    }

    private func showInFinder(_ item: DownloadItem) {
        let fileURL = item.destinationDirectory.appendingPathComponent(item.fileName)
        if FileManager.default.fileExists(atPath: fileURL.path) {
            NSWorkspace.shared.activateFileViewerSelecting([fileURL])
        } else {
            NSWorkspace.shared.open(existingFolder(for: item.destinationDirectory))
        }
    }

    private func existingFolder(for url: URL) -> URL {
        var candidate = url
        while !FileManager.default.fileExists(atPath: candidate.path) {
            let parent = candidate.deletingLastPathComponent()
            if parent.path == candidate.path {
                return URL(fileURLWithPath: NSHomeDirectory())
            }
            candidate = parent
        }
        return candidate
    }
}

private struct DownloadRow: View {
    let item: DownloadItem
    let visibleColumns: [DownloadColumn]
    let columnWidth: (DownloadColumn) -> CGFloat
    let trailingWidth: CGFloat

    var body: some View {
        HStack(alignment: .top, spacing: 0) {
            ForEach(visibleColumns) { column in
                cell(for: column)
                    .padding(.horizontal, 8)
                    .padding(.vertical, 8)
                    .frame(width: columnWidth(column), alignment: alignment(for: column))
                    .clipped()
            }

            if trailingWidth > 0 {
                Color.clear
                    .frame(width: trailingWidth)
            }
        }
    }

    @ViewBuilder
    private func cell(for column: DownloadColumn) -> some View {
        switch column {
        case .fileName:
            HStack(alignment: .top, spacing: 8) {
                Image(systemName: item.category.symbolName)
                    .font(.title3)
                    .foregroundStyle(categoryColor)
                    .frame(width: 22)
                VStack(alignment: .leading, spacing: 6) {
                    Text(item.fileName)
                        .font(.headline)
                        .lineLimit(1)
                        .truncationMode(.tail)
                    Text(item.url.absoluteString)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                        .truncationMode(.tail)
                }
                .frame(maxWidth: .infinity, alignment: .leading)
            }
        case .size:
            Text(ByteFormatter.string(item.sizeBytes))
                .monospacedDigit()
                .lineLimit(1)
                .truncationMode(.tail)
        case .status:
            Text(item.status.rawValue)
                .lineLimit(1)
                .truncationMode(.tail)
        case .progress:
            Text(progressStatusText)
                .monospacedDigit()
                .foregroundStyle(statusColor)
                .lineLimit(1)
                .truncationMode(.tail)
        case .lastTry:
            Text(formatted(item.lastTryAt))
                .lineLimit(1)
                .truncationMode(.tail)
        case .dateAdded:
            Text(formatted(item.createdAt))
                .lineLimit(1)
                .truncationMode(.tail)
        case .category:
            Text(item.category.rawValue)
                .lineLimit(1)
                .truncationMode(.tail)
        case .connections:
            Text("\(item.connectionsPerServer)")
                .monospacedDigit()
                .lineLimit(1)
        case .liveConnections:
            Text("\(item.connectionCount)")
                .monospacedDigit()
                .lineLimit(1)
        case .speed:
            Text(item.speedText)
                .lineLimit(1)
                .truncationMode(.tail)
        case .eta:
            Text(item.etaText)
                .lineLimit(1)
                .truncationMode(.tail)
        case .destination:
            Text(item.destinationPath)
                .font(.system(.caption, design: .monospaced))
                .lineLimit(1)
                .truncationMode(.tail)
        case .url:
            Text(item.url.absoluteString)
                .font(.system(.caption, design: .monospaced))
                .lineLimit(1)
                .truncationMode(.tail)
        case .message:
            Text(item.message)
                .foregroundStyle(.secondary)
                .lineLimit(1)
                .truncationMode(.tail)
        }
    }

    private func alignment(for column: DownloadColumn) -> Alignment {
        column == .fileName ? .leading : .center
    }

    private var categoryColor: Color {
        switch item.category {
        case .musics: .pink
        case .movies: .indigo
        case .compressed: .orange
        case .pictures: .teal
        case .documents: .blue
        case .other: .gray
        }
    }

    private var statusColor: Color {
        switch item.status {
        case .queued: .secondary
        case .downloading: .blue
        case .paused: .orange
        case .completed: .green
        case .failed: .red
        case .canceled: .gray
        }
    }

    private var progressStatusText: String {
        switch item.status {
        case .completed:
            "Completed"
        case .failed:
            "Failed"
        case .canceled:
            "Canceled"
        case .paused:
            "Paused \(item.progress.formatted(.percent.precision(.fractionLength(0))))"
        case .queued:
            "Queued"
        case .downloading:
            item.progress.formatted(.percent.precision(.fractionLength(0)))
        }
    }

    private func formatted(_ date: Date?) -> String {
        guard let date else { return "-" }
        return date.formatted(date: .abbreviated, time: .shortened)
    }
}

private struct StatusBar: View {
    @EnvironmentObject private var controller: DownloadController

    var body: some View {
        HStack {
            Text(controller.engineMessage.isEmpty ? "Ready" : controller.engineMessage)
                .foregroundStyle(controller.hasAria2 ? Color.secondary : Color.orange)
                .lineLimit(1)
            Spacer()
            Text("\(controller.activeCount) active")
            Text("\(controller.queuedCount) queued")
            Text("\(controller.completedCount) done")
        }
        .font(.caption)
        .padding(.horizontal, 14)
        .padding(.vertical, 8)
        .background(.bar)
    }
}

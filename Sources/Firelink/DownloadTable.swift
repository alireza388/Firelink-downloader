import AppKit
import SwiftUI

enum DownloadColumn: String, CaseIterable, Identifiable, Codable {
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
        case .fileName: return 340
        case .size: return 100
        case .status: return 105
        case .progress: return 115
        case .lastTry, .dateAdded: return 155
        case .category: return 105
        case .connections, .liveConnections: return 95
        case .speed, .eta: return 90
        case .destination: return 240
        case .url: return 280
        case .message: return 220
        }
    }
}

enum SortDirection: String, Codable {
    case ascending
    case descending

    mutating func toggle() {
        self = self == .ascending ? .descending : .ascending
    }
}

final class TableSettings: ObservableObject {
    @Published var visibleColumns: Set<DownloadColumn> {
        didSet { save() }
    }
    @Published var columnWidths: [DownloadColumn: CGFloat] {
        didSet { save() }
    }
    @Published var sortColumn: DownloadColumn {
        didSet { save() }
    }
    @Published var sortDirection: SortDirection {
        didSet { save() }
    }

    private let defaults = UserDefaults.standard
    private let storageKey = "Firelink.TableSettings.v1"

    init() {
        if let data = defaults.data(forKey: storageKey),
           let stored = try? JSONDecoder().decode(StoredTableSettings.self, from: data) {
            visibleColumns = stored.visibleColumns
            columnWidths = stored.columnWidths
            sortColumn = stored.sortColumn
            sortDirection = stored.sortDirection
        } else {
            visibleColumns = [.fileName, .size, .progress, .eta, .lastTry, .dateAdded]
            columnWidths = Dictionary(uniqueKeysWithValues: DownloadColumn.allCases.map { ($0, $0.width) })
            sortColumn = .dateAdded
            sortDirection = .descending
        }
    }

    private func save() {
        let stored = StoredTableSettings(
            visibleColumns: visibleColumns,
            columnWidths: columnWidths,
            sortColumn: sortColumn,
            sortDirection: sortDirection
        )
        if let data = try? JSONEncoder().encode(stored) {
            defaults.set(data, forKey: storageKey)
        }
    }
}

private struct StoredTableSettings: Codable {
    var visibleColumns: Set<DownloadColumn>
    var columnWidths: [DownloadColumn: CGFloat]
    var sortColumn: DownloadColumn
    var sortDirection: SortDirection
}

struct DownloadTable: View {
    @EnvironmentObject private var controller: DownloadController
    @EnvironmentObject private var settings: AppSettings
    @Environment(\.openWindow) private var openWindow
    let items: [DownloadItem]
    @Binding var selection: DownloadItem.ID?
    let title: String

    @StateObject private var tableSettings = TableSettings()
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
                        if tableSettings.sortColumn == column {
                            tableSettings.sortDirection.toggle()
                        } else {
                            tableSettings.sortColumn = column
                            tableSettings.sortDirection = .ascending
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
                            if tableSettings.sortColumn == column {
                                Image(systemName: tableSettings.sortDirection == .ascending ? "chevron.up" : "chevron.down")
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
                                    tableSettings.columnWidths[column] = max(70, baseWidth + value.translation.width)
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
                        get: { tableSettings.visibleColumns.contains(column) },
                        set: { isVisible in
                            if isVisible {
                                tableSettings.visibleColumns.insert(column)
                            } else if tableSettings.visibleColumns.count > 1 {
                                tableSettings.visibleColumns.remove(column)
                            }
                        }
                    ))
                }
            }
            Section("Sort By") {
                ForEach(DownloadColumn.allCases) { column in
                    Button(column.rawValue) {
                        tableSettings.sortColumn = column
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
        DownloadColumn.allCases.filter { tableSettings.visibleColumns.contains($0) }
    }

    private var totalWidth: CGFloat {
        orderedVisibleColumns.map { width(for: $0) }.reduce(0, +)
    }

    private func width(for column: DownloadColumn) -> CGFloat {
        max(70, tableSettings.columnWidths[column] ?? column.width)
    }

    private var sortedItems: [DownloadItem] {
        items.sorted { lhs, rhs in
            let result = compare(lhs, rhs, by: tableSettings.sortColumn)
            if result == .orderedSame {
                return lhs.id.uuidString < rhs.id.uuidString
            }
            return tableSettings.sortDirection == .ascending ? result == .orderedAscending : result == .orderedDescending
        }
    }

    private func compare(_ lhs: DownloadItem, _ rhs: DownloadItem, by column: DownloadColumn) -> ComparisonResult {
        switch column {
        case .fileName: return lhs.fileName.localizedCaseInsensitiveCompare(rhs.fileName)
        case .size: return compare(lhs.sizeBytes ?? -1, rhs.sizeBytes ?? -1)
        case .status: return compare(lhs.status.rawValue, rhs.status.rawValue)
        case .progress: return compare(lhs.progress, rhs.progress)
        case .lastTry: return compare(lhs.lastTryAt ?? .distantPast, rhs.lastTryAt ?? .distantPast)
        case .dateAdded: return compare(lhs.createdAt, rhs.createdAt)
        case .category: return compare(lhs.category.rawValue, rhs.category.rawValue)
        case .connections: return compare(lhs.connectionsPerServer, rhs.connectionsPerServer)
        case .liveConnections: return compare(lhs.connectionCount, rhs.connectionCount)
        case .speed: return compare(lhs.speedText, rhs.speedText)
        case .eta: return compare(lhs.etaText, rhs.etaText)
        case .destination: return compare(lhs.destinationPath, rhs.destinationPath)
        case .url: return compare(lhs.url.absoluteString, rhs.url.absoluteString)
        case .message: return compare(lhs.message, rhs.message)
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
        case .musics: return .pink
        case .movies: return .indigo
        case .compressed: return .orange
        case .pictures: return .teal
        case .documents: return .blue
        case .other: return .gray
        }
    }

    private var statusColor: Color {
        switch item.status {
        case .queued: return .secondary
        case .downloading: return .blue
        case .paused: return .orange
        case .completed: return .green
        case .failed: return .red
        case .canceled: return .gray
        }
    }

    private var progressStatusText: String {
        switch item.status {
        case .completed:
            return "Completed"
        case .failed:
            return "Failed"
        case .canceled:
            return "Canceled"
        case .paused:
            return "Paused \(item.progress.formatted(.percent.precision(.fractionLength(0))))"
        case .queued:
            return "Queued"
        case .downloading:
            return item.progress.formatted(.percent.precision(.fractionLength(0)))
        }
    }

    private func formatted(_ date: Date?) -> String {
        guard let date else { return "-" }
        return date.formatted(date: .abbreviated, time: .shortened)
    }
}

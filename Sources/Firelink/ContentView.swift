import SwiftUI

struct ContentView: View {
    @EnvironmentObject private var controller: DownloadController
    @State private var urlText = ""
    @State private var username = ""
    @State private var password = ""
    @State private var parts = 16.0
    @State private var selection: DownloadItem.ID?

    var body: some View {
        NavigationSplitView {
            SidebarView()
                .navigationSplitViewColumnWidth(min: 190, ideal: 220, max: 260)
        } detail: {
            VStack(spacing: 0) {
                AddDownloadBar(
                    urlText: $urlText,
                    username: $username,
                    password: $password,
                    parts: $parts,
                    addAction: addDownload
                )
                Divider()
                DownloadTable(selection: $selection)
                Divider()
                StatusBar()
            }
            .toolbar {
                ToolbarItemGroup {
                    Button {
                        controller.startQueue()
                    } label: {
                        Label("Start Queue", systemImage: "play.fill")
                    }

                    if let selectedItem {
                        if selectedItem.status == .downloading {
                            Button {
                                controller.pause(selectedItem)
                            } label: {
                                Label("Pause", systemImage: "pause.fill")
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

    private func addDownload() {
        controller.add(
            urlText: urlText,
            parts: Int(parts),
            username: username,
            password: password
        )
        if controller.engineMessage.hasPrefix("Added") {
            urlText = ""
            username = ""
            password = ""
        }
    }
}

private struct SidebarView: View {
    @EnvironmentObject private var controller: DownloadController

    var body: some View {
        List {
            Section("Library") {
                Label("Queue", systemImage: "list.bullet")
                    .badge(controller.queuedCount)
                Label("Active", systemImage: "bolt.fill")
                    .badge(controller.activeCount)
                Label("Completed", systemImage: "checkmark.circle")
                    .badge(controller.completedCount)
                Label("Failed", systemImage: "exclamationmark.triangle")
                    .badge(controller.failedCount)
            }

            Section("Folders") {
                ForEach(DownloadCategory.allCases, id: \.self) { category in
                    Label(category.rawValue, systemImage: category.symbolName)
                }
            }

            Section("Engine") {
                HStack {
                    Image(systemName: controller.hasAria2 ? "checkmark.seal.fill" : "exclamationmark.triangle.fill")
                        .foregroundStyle(controller.hasAria2 ? .green : .orange)
                    Text(controller.hasAria2 ? "aria2c ready" : "aria2c missing")
                }
                Stepper("Parallel files: \(controller.maxConcurrentDownloads)", value: $controller.maxConcurrentDownloads, in: 1...12)
            }
        }
        .listStyle(.sidebar)
    }
}

private struct AddDownloadBar: View {
    @Binding var urlText: String
    @Binding var username: String
    @Binding var password: String
    @Binding var parts: Double
    let addAction: () -> Void

    var body: some View {
        VStack(spacing: 12) {
            HStack(spacing: 10) {
                TextField("Download URL", text: $urlText)
                    .textFieldStyle(.roundedBorder)
                    .font(.system(.body, design: .monospaced))

                Button(action: addAction) {
                    Label("Add", systemImage: "plus")
                }
                .buttonStyle(.borderedProminent)
                .disabled(urlText.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
            }

            HStack(spacing: 12) {
                HStack {
                    Image(systemName: "square.split.2x2")
                    Slider(value: $parts, in: 16...32, step: 1)
                        .frame(width: 160)
                    Text("\(Int(parts)) parts")
                        .monospacedDigit()
                        .frame(width: 62, alignment: .trailing)
                }

                Divider()
                    .frame(height: 22)

                TextField("Username", text: $username)
                    .textFieldStyle(.roundedBorder)
                    .frame(width: 180)
                SecureField("Password", text: $password)
                    .textFieldStyle(.roundedBorder)
                    .frame(width: 180)

                Spacer()
            }
            .font(.callout)
        }
        .padding(14)
        .background(.background)
    }
}

private struct DownloadTable: View {
    @EnvironmentObject private var controller: DownloadController
    @Binding var selection: DownloadItem.ID?

    var body: some View {
        List(selection: $selection) {
            ForEach(controller.downloads) { item in
                DownloadRow(item: item)
                    .tag(item.id)
            }
            .onMove(perform: controller.move)
            .onDelete(perform: controller.remove)
        }
        .listStyle(.inset)
    }
}

private struct DownloadRow: View {
    let item: DownloadItem

    var body: some View {
        Grid(alignment: .leading, horizontalSpacing: 14, verticalSpacing: 8) {
            GridRow {
                Image(systemName: item.category.symbolName)
                    .font(.title3)
                    .foregroundStyle(categoryColor)
                    .frame(width: 24)

                VStack(alignment: .leading, spacing: 6) {
                    HStack {
                        Text(item.fileName)
                            .font(.headline)
                            .lineLimit(1)
                        Text(item.status.rawValue)
                            .font(.caption)
                            .padding(.horizontal, 7)
                            .padding(.vertical, 3)
                            .background(statusColor.opacity(0.14))
                            .foregroundStyle(statusColor)
                            .clipShape(RoundedRectangle(cornerRadius: 5))
                    }

                    ProgressView(value: item.progress)
                        .progressViewStyle(.linear)

                    Text(item.url.absoluteString)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                }

                metric("Parts", "\(item.parts)")
                metric("CN", "\(item.connectionCount)")
                metric("Speed", item.speedText)
                metric("ETA", item.etaText)
                metric("Size", item.bytesText)

                Text(item.message)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(2)
                    .frame(width: 220, alignment: .leading)
            }
        }
        .padding(.vertical, 7)
    }

    private func metric(_ label: String, _ value: String) -> some View {
        VStack(alignment: .trailing, spacing: 2) {
            Text(label)
                .font(.caption2)
                .foregroundStyle(.secondary)
            Text(value)
                .font(.callout.monospacedDigit())
                .lineLimit(1)
        }
        .frame(width: 72, alignment: .trailing)
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

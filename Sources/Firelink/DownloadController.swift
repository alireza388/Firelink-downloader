import Combine
import Foundation

@MainActor
final class DownloadController: ObservableObject {
    @Published var downloads: [DownloadItem] = []
    @Published var maxConcurrentDownloads = 3
    @Published var engineMessage = ""

    private let engine = Aria2DownloadEngine()
    private var activeHandles: [UUID: Aria2DownloadEngine.Handle] = [:]

    var activeCount: Int {
        downloads.filter { $0.status == .downloading }.count
    }

    var queuedCount: Int {
        downloads.filter { $0.status == .queued }.count
    }

    var completedCount: Int {
        downloads.filter { $0.status == .completed }.count
    }

    var failedCount: Int {
        downloads.filter { $0.status == .failed }.count
    }

    var hasAria2: Bool {
        Aria2DownloadEngine.findExecutable() != nil
    }

    func add(urlText: String, parts: Int, username: String, password: String) {
        guard let url = URL(string: urlText.trimmingCharacters(in: .whitespacesAndNewlines)),
              let scheme = url.scheme?.lowercased(),
              ["http", "https", "ftp", "sftp"].contains(scheme) else {
            engineMessage = "Enter a valid HTTP, HTTPS, FTP, or SFTP URL."
            return
        }

        let fileName = FileClassifier.fileName(from: url)
        let category = FileClassifier.category(forFileName: fileName)
        let credentials = DownloadCredentials(username: username, password: password)
        let item = DownloadItem(
            url: url,
            fileName: fileName,
            category: category,
            destinationDirectory: FileClassifier.destinationDirectory(for: category),
            parts: min(max(parts, 16), 32),
            credentials: credentials.isEmpty ? nil : credentials
        )

        downloads.append(item)
        engineMessage = "Added \(fileName) to \(category.rawValue)."
    }

    func startQueue() {
        engineMessage = ""
        pumpQueue()
    }

    func pause(_ item: DownloadItem) {
        activeHandles[item.id]?.cancel()
        activeHandles[item.id] = nil
        update(item.id) {
            $0.status = .paused
            $0.message = "Paused. Resume will continue from the partial file."
        }
        pumpQueue()
    }

    func resume(_ item: DownloadItem) {
        update(item.id) {
            $0.status = .queued
            $0.message = ""
        }
        pumpQueue()
    }

    func cancel(_ item: DownloadItem) {
        activeHandles[item.id]?.cancel()
        activeHandles[item.id] = nil
        update(item.id) {
            $0.status = .canceled
            $0.message = "Canceled"
        }
        pumpQueue()
    }

    func remove(at offsets: IndexSet) {
        for index in offsets {
            let item = downloads[index]
            activeHandles[item.id]?.cancel()
            activeHandles[item.id] = nil
        }
        downloads.remove(atOffsets: offsets)
    }

    func move(from source: IndexSet, to destination: Int) {
        downloads.move(fromOffsets: source, toOffset: destination)
    }

    private func pumpQueue() {
        guard hasAria2 else {
            engineMessage = "aria2c is not installed. Run `brew install aria2` to enable downloads."
            return
        }

        while activeCount < maxConcurrentDownloads,
              let next = downloads.first(where: { $0.status == .queued }) {
            start(next)
        }
    }

    private func start(_ item: DownloadItem) {
        update(item.id) {
            $0.status = .downloading
            $0.message = "Starting"
            $0.speedText = "-"
            $0.etaText = "-"
        }

        do {
            let handle = try engine.start(
                item: item,
                progress: { [weak self] progress in
                    Task { @MainActor in
                        self?.update(item.id) {
                            $0.progress = progress.fraction
                            $0.bytesText = progress.bytesText
                            $0.speedText = progress.speedText
                            $0.etaText = progress.etaText
                            $0.connectionCount = progress.connectionCount
                            $0.message = "Downloading"
                        }
                    }
                },
                completion: { [weak self] result in
                    Task { @MainActor in
                        guard let self else { return }
                        self.activeHandles[item.id] = nil

                        switch result {
                        case .success:
                            self.update(item.id) {
                                $0.status = .completed
                                $0.progress = 1
                                $0.speedText = "-"
                                $0.etaText = "-"
                                $0.message = "Saved to \($0.destinationPath)"
                            }
                        case .failure(let error):
                            if self.downloads.first(where: { $0.id == item.id })?.status == .paused ||
                                self.downloads.first(where: { $0.id == item.id })?.status == .canceled {
                                return
                            }
                            self.update(item.id) {
                                $0.status = .failed
                                $0.message = error.localizedDescription
                            }
                        }

                        self.pumpQueue()
                    }
                }
            )
            activeHandles[item.id] = handle
            update(item.id) {
                $0.message = "Process \(handle.processIdentifier)"
            }
        } catch {
            update(item.id) {
                $0.status = .failed
                $0.message = error.localizedDescription
            }
            pumpQueue()
        }
    }

    private func update(_ id: UUID, mutate: (inout DownloadItem) -> Void) {
        guard let index = downloads.firstIndex(where: { $0.id == id }) else { return }
        mutate(&downloads[index])
    }
}

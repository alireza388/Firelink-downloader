import AppKit
import Combine
import Foundation

@MainActor
final class DownloadController: ObservableObject {
    @Published var downloads: [DownloadItem] = []
    @Published var engineMessage = ""

    private let settings: AppSettings
    private let engine = Aria2DownloadEngine()
    private var activeHandles: [UUID: Aria2DownloadEngine.Handle] = [:]
    private var sleepActivity: SleepActivityHandle?
    private var cancellables = Set<AnyCancellable>()
    private lazy var storageURL: URL = {
        let supportDir = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first ?? URL(fileURLWithPath: NSHomeDirectory())
        return supportDir.appendingPathComponent("Firelink").appendingPathComponent("downloads.json")
    }()

    init(settings: AppSettings) {
        self.settings = settings
        
        loadDownloads()
        
        settings.$preventsSleepWhileDownloading
            .sink { [weak self] _ in
                Task { @MainActor in
                    self?.updateSleepActivity()
                }
            }
            .store(in: &cancellables)
            
        $downloads
            .dropFirst()
            .debounce(for: .seconds(2.0), scheduler: RunLoop.main)
            .sink { [weak self] _ in
                self?.saveDownloads()
            }
            .store(in: &cancellables)
            
        NotificationCenter.default.publisher(for: NSApplication.willTerminateNotification)
            .sink { [weak self] _ in
                self?.saveDownloads()
            }
            .store(in: &cancellables)
    }

    deinit {
        sleepActivity?.end()
    }

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

    func add(urlText: String, connectionsPerServer: Int? = nil) {
        guard let url = URL(string: urlText.trimmingCharacters(in: .whitespacesAndNewlines)),
              let scheme = url.scheme?.lowercased(),
              ["http", "https", "ftp", "sftp"].contains(scheme) else {
            engineMessage = "Enter a valid HTTP, HTTPS, FTP, or SFTP URL."
            return
        }

        let fileName = FileClassifier.fileName(from: url)
        let category = FileClassifier.category(forFileName: fileName)
        let item = DownloadItem(
            url: url,
            fileName: fileName,
            category: category,
            destinationDirectory: settings.destinationDirectory(for: category),
            connectionsPerServer: min(max(connectionsPerServer ?? settings.perServerConnections, 1), 16),
            credentials: settings.credentials(for: url)
        )

        downloads.append(item)
        engineMessage = "Added \(fileName) to \(category.rawValue)."
    }

    func addPendingDownloads(
        _ pendingDownloads: [PendingDownload],
        connectionsPerServer: Int,
        overrideDirectory: URL?,
        startImmediately: Bool
    ) {
        let clampedConnections = min(max(connectionsPerServer, 1), 16)

        let items = pendingDownloads.map { pending in
            DownloadItem(
                url: pending.url,
                fileName: pending.fileName,
                category: pending.category,
                destinationDirectory: overrideDirectory ?? pending.defaultDirectory,
                connectionsPerServer: clampedConnections,
                credentials: settings.credentials(for: pending.url),
                sizeBytes: pending.sizeBytes,
                bytesText: ByteFormatter.string(pending.sizeBytes),
                message: startImmediately ? "Queued to start" : "Added to queue"
            )
        }

        downloads.append(contentsOf: items)
        engineMessage = "Added \(items.count) download\(items.count == 1 ? "" : "s")."

        if startImmediately {
            startQueue()
        }
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
        updateSleepActivity()
        pumpQueue()
    }

    func queue(_ item: DownloadItem) {
        activeHandles[item.id]?.cancel()
        activeHandles[item.id] = nil
        update(item.id) {
            $0.status = .queued
            if item.status != .paused {
                $0.progress = 0
                $0.speedText = "-"
                $0.etaText = "-"
                $0.connectionCount = 0
            }
            $0.message = "Added to queue"
        }
        updateSleepActivity()
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
        updateSleepActivity()
        pumpQueue()
    }

    func remove(at offsets: IndexSet, deleteFiles: Bool = false) {
        for index in offsets {
            let item = downloads[index]
            delete(item, deleteFiles: deleteFiles)
        }
    }

    func delete(_ item: DownloadItem, deleteFiles: Bool = false) {
        activeHandles[item.id]?.cancel()
        activeHandles[item.id] = nil
        if deleteFiles {
            trashFiles(for: item)
        } else if item.status != .completed {
            removeCacheFiles(for: item)
        }
        downloads.removeAll { $0.id == item.id }
        updateSleepActivity()
    }

    func move(from source: IndexSet, to destination: Int) {
        downloads.move(fromOffsets: source, toOffset: destination)
    }

    private func pumpQueue() {
        guard hasAria2 else {
            engineMessage = "aria2c is not installed. Run `brew install aria2` to enable downloads."
            return
        }

        while activeCount < settings.maxConcurrentDownloads,
              let next = downloads.first(where: { $0.status == .queued }) {
            start(next)
        }
    }

    private func start(_ item: DownloadItem) {
        update(item.id) {
            $0.status = .downloading
            $0.lastTryAt = Date()
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
                        self.updateSleepActivity()
                    }
                }
            )
            activeHandles[item.id] = handle
            update(item.id) {
                $0.message = "Process \(handle.processIdentifier)"
            }
            updateSleepActivity()
        } catch {
            update(item.id) {
                $0.status = .failed
                $0.message = error.localizedDescription
            }
            updateSleepActivity()
            pumpQueue()
        }
    }

    private func update(_ id: UUID, mutate: (inout DownloadItem) -> Void) {
        guard let index = downloads.firstIndex(where: { $0.id == id }) else { return }
        mutate(&downloads[index])
    }

    func updateDownload(
        id: UUID,
        url: URL,
        fileName: String,
        destinationDirectory: URL,
        connectionsPerServer: Int,
        credentials: DownloadCredentials?
    ) {
        update(id) {
            $0.url = url
            $0.fileName = fileName
            $0.category = FileClassifier.category(forFileName: fileName)
            $0.destinationDirectory = destinationDirectory
            $0.connectionsPerServer = min(max(connectionsPerServer, 1), 16)
            $0.credentials = credentials
            $0.message = "Properties updated"
        }
    }

    private func updateSleepActivity() {
        let shouldPreventSleep = settings.preventsSleepWhileDownloading && activeCount > 0

        if shouldPreventSleep, sleepActivity == nil {
            sleepActivity = SleepActivityHandle(activity: ProcessInfo.processInfo.beginActivity(
                options: [.idleSystemSleepDisabled],
                reason: "Firelink is downloading files."
            ))
        } else if !shouldPreventSleep, let activity = sleepActivity {
            activity.end()
            sleepActivity = nil
        }
    }

    private func removeCacheFiles(for item: DownloadItem) {
        let fileURL = item.destinationDirectory.appendingPathComponent(item.fileName)
        let candidates = [URL(fileURLWithPath: fileURL.path + ".aria2")]

        for candidate in candidates where FileManager.default.fileExists(atPath: candidate.path) {
            try? FileManager.default.removeItem(at: candidate)
        }
    }

    private func trashFiles(for item: DownloadItem) {
        let fileURL = item.destinationDirectory.appendingPathComponent(item.fileName)
        let candidates = [
            fileURL,
            URL(fileURLWithPath: fileURL.path + ".aria2")
        ]

        for candidate in candidates where FileManager.default.fileExists(atPath: candidate.path) {
            try? FileManager.default.trashItem(at: candidate, resultingItemURL: nil)
        }
    }

    private func saveDownloads() {
        do {
            let directory = storageURL.deletingLastPathComponent()
            try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true, attributes: nil)
            let data = try JSONEncoder().encode(downloads)
            try data.write(to: storageURL, options: .atomic)
        } catch {
            print("Failed to save downloads: \(error)")
        }
    }

    private func loadDownloads() {
        do {
            guard FileManager.default.fileExists(atPath: storageURL.path) else { return }
            let data = try Data(contentsOf: storageURL)
            let loaded = try JSONDecoder().decode([DownloadItem].self, from: data)
            
            self.downloads = loaded.map { item in
                var adjusted = item
                if adjusted.status == .downloading {
                    adjusted.status = .paused
                    adjusted.message = "Paused on startup"
                    adjusted.speedText = "-"
                    adjusted.etaText = "-"
                    adjusted.connectionCount = 0
                }
                return adjusted
            }
        } catch {
            print("Failed to load downloads: \(error)")
        }
    }
}

private final class SleepActivityHandle: @unchecked Sendable {
    private let activity: NSObjectProtocol

    init(activity: NSObjectProtocol) {
        self.activity = activity
    }

    func end() {
        ProcessInfo.processInfo.endActivity(activity)
    }
}

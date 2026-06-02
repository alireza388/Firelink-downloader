import Foundation

enum DownloadStatus: String, Codable, CaseIterable, Sendable {
    case queued = "Queued"
    case downloading = "Downloading"
    case paused = "Paused"
    case completed = "Completed"
    case failed = "Failed"
    case canceled = "Canceled"
}

enum DownloadCategory: String, Codable, CaseIterable, Sendable {
    case musics = "Musics"
    case movies = "Movies"
    case compressed = "Compressed"
    case pictures = "Pictures"
    case documents = "Documents"
    case other = "Other"

    var symbolName: String {
        switch self {
        case .musics: "music.note"
        case .movies: "film"
        case .compressed: "archivebox"
        case .pictures: "photo"
        case .documents: "doc.text"
        case .other: "folder"
        }
    }
}

struct DownloadCredentials: Codable, Equatable, Sendable {
    var username: String
    var password: String

    var isEmpty: Bool {
        username.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty &&
            password.isEmpty
    }
}

struct DownloadItem: Identifiable, Codable, Equatable, Sendable {
    var id = UUID()
    var url: URL
    var fileName: String
    var category: DownloadCategory
    var destinationDirectory: URL
    var connectionsPerServer: Int
    var credentials: DownloadCredentials?
    var status: DownloadStatus = .queued
    var progress: Double = 0
    var speedText: String = "-"
    var etaText: String = "-"
    var connectionCount: Int = 0
    var sizeBytes: Int64?
    var bytesText: String = "-"
    var message: String = ""
    var createdAt = Date()
    var lastTryAt: Date?
    var autoResumeOnLaunch: Bool?

    var destinationPath: String {
        destinationDirectory.appendingPathComponent(fileName).path
    }
}

struct DownloadProgress: Equatable, Sendable {
    var fraction: Double
    var bytesText: String
    var speedText: String
    var etaText: String
    var connectionCount: Int
}

struct PendingDownload: Identifiable, Equatable, Sendable {
    enum MetadataState: Equatable, Sendable {
        case pending
        case loading
        case loaded
        case failed(String)
    }

    var id = UUID()
    var url: URL
    var fileName: String
    var category: DownloadCategory
    var defaultDirectory: URL
    var sizeBytes: Int64?
    var mimeType: String?
    var state: MetadataState = .pending

    var destinationPath: String {
        defaultDirectory.appendingPathComponent(fileName).path
    }
}

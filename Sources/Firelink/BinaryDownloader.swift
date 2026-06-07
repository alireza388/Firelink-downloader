import Foundation

enum BinaryDownloaderError: Error {
    case invalidResponse
    case httpError(statusCode: Int)
    case downloadFailed(Error?)
    case moveFailed(Error)
    case permissionFailed(Error)
    case unzipFailed
}

final class BinaryDownloader: NSObject, URLSessionDownloadDelegate, Sendable {
    private let url: URL
    private let destination: URL
    private let onProgress: @Sendable (Double) -> Void
    private let session: URLSession
    
    private let continuation: CheckedContinuation<Void, Error>
    
    init(url: URL, destination: URL, onProgress: @escaping @Sendable (Double) -> Void, continuation: CheckedContinuation<Void, Error>) {
        self.url = url
        self.destination = destination
        self.onProgress = onProgress
        self.continuation = continuation
        
        let config = URLSessionConfiguration.ephemeral
        self.session = URLSession(configuration: config, delegate: nil, delegateQueue: nil) // Delegate set below
        super.init()
    }
    
    static func download(from url: URL, to destination: URL, onProgress: @escaping @Sendable (Double) -> Void) async throws {
        try await withCheckedThrowingContinuation { continuation in
            let downloader = BinaryDownloader(url: url, destination: destination, onProgress: onProgress, continuation: continuation)
            let session = URLSession(configuration: .ephemeral, delegate: downloader, delegateQueue: nil)
            let task = session.downloadTask(with: url)
            task.resume()
        }
    }
    
    func urlSession(_ session: URLSession, downloadTask: URLSessionDownloadTask, didFinishDownloadingTo location: URL) {
        defer { session.finishTasksAndInvalidate() }
        guard let response = downloadTask.response as? HTTPURLResponse else {
            continuation.resume(throwing: BinaryDownloaderError.invalidResponse)
            return
        }
        guard (200...299).contains(response.statusCode) else {
            continuation.resume(throwing: BinaryDownloaderError.httpError(statusCode: response.statusCode))
            return
        }
        
        do {
            if FileManager.default.fileExists(atPath: destination.path) {
                try FileManager.default.removeItem(at: destination)
            }
            try FileManager.default.moveItem(at: location, to: destination)
            
            // Make executable
            try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: destination.path)
            
            continuation.resume()
        } catch {
            continuation.resume(throwing: BinaryDownloaderError.moveFailed(error))
        }
    }
    
    func urlSession(_ session: URLSession, downloadTask: URLSessionDownloadTask, didWriteData bytesWritten: Int64, totalBytesWritten: Int64, totalBytesExpectedToWrite: Int64) {
        guard totalBytesExpectedToWrite > 0 else { return }
        let progress = Double(totalBytesWritten) / Double(totalBytesExpectedToWrite)
        onProgress(progress)
    }
    
    func urlSession(_ session: URLSession, task: URLSessionTask, didCompleteWithError error: Error?) {
        if let error = error {
            session.finishTasksAndInvalidate()
            continuation.resume(throwing: BinaryDownloaderError.downloadFailed(error))
        }
    }
}

import Foundation

struct RawMediaFormat: Decodable, Sendable {
    let format_id: String?
    let ext: String?
    let resolution: String?
    let format_note: String?
    let vcodec: String?
    let acodec: String?
    let height: Int?
    let filesize: Int64?
    let filesize_approx: Int64?
}

struct MediaMetadata: Decodable, Sendable {
    let id: String?
    let title: String?
    let uploader: String?
    let channel: String?
    let thumbnail: URL?
    let duration: Double?
    let formats: [RawMediaFormat]?
    
    var displayUploader: String? {
        channel ?? uploader
    }
}

struct CleanFormatOption: Identifiable, Equatable, Sendable {
    var id: String { name }
    let name: String
    let formatSelector: String
    let isAudioOnly: Bool
    let symbol: String
}

enum MediaExtractionEngine {
    enum ExtractionError: Error, LocalizedError {
        case processFailed(String)
        case invalidOutput
        case parsingFailed(Error)
        
        var errorDescription: String? {
            switch self {
            case .processFailed(let msg): return "Extraction failed: \(msg)"
            case .invalidOutput: return "Invalid output from media engine."
            case .parsingFailed(let err): return "Failed to parse metadata: \(err.localizedDescription)"
            }
        }
    }
    
    static func fetchMetadata(for url: URL) async throws -> (MediaMetadata, [CleanFormatOption]) {
        let ytDlpPath = await MediaEngineManager.shared.binaryPath(for: .ytDlp).path
        guard FileManager.default.fileExists(atPath: ytDlpPath) else {
            throw ExtractionError.processFailed("yt-dlp binary not found.")
        }
        
        let process = Process()
        process.executableURL = URL(fileURLWithPath: ytDlpPath)
        
        var args = ["-J", "--no-warnings", "--ignore-no-formats-error"]
        
        // Add cookies if configured
        if let storedData = UserDefaults.standard.data(forKey: "Firelink.AppSettings.v1"),
           let json = try? JSONSerialization.jsonObject(with: storedData) as? [String: Any],
           let cookieSourceStr = json["mediaCookieSource"] as? String,
           cookieSourceStr != "None" {
            args.append(contentsOf: ["--cookies-from-browser", cookieSourceStr.lowercased()])
        }
        
        args.append(url.absoluteString)
        process.arguments = args
        
        let pipe = Pipe()
        let errorPipe = Pipe()
        process.standardOutput = pipe
        process.standardError = errorPipe
        
        return try await withCheckedThrowingContinuation { continuation in
            do {
                try process.run()
            } catch {
                continuation.resume(throwing: ExtractionError.processFailed(error.localizedDescription))
                return
            }
            
            process.terminationHandler = { p in
                let data = pipe.fileHandleForReading.readDataToEndOfFile()
                let errorData = errorPipe.fileHandleForReading.readDataToEndOfFile()
                
                if p.terminationStatus != 0 {
                    let errorString = String(data: errorData, encoding: .utf8) ?? "Unknown error"
                    continuation.resume(throwing: ExtractionError.processFailed(errorString))
                    return
                }
                
                guard !data.isEmpty else {
                    continuation.resume(throwing: ExtractionError.invalidOutput)
                    return
                }
                
                do {
                    let metadata = try JSONDecoder().decode(MediaMetadata.self, from: data)
                    let options = extractOptions(from: metadata)
                    continuation.resume(returning: (metadata, options))
                } catch {
                    continuation.resume(throwing: ExtractionError.parsingFailed(error))
                }
            }
        }
    }
    
    private static func extractOptions(from metadata: MediaMetadata) -> [CleanFormatOption] {
        var options: [CleanFormatOption] = []
        let rawFormats = metadata.formats ?? []
        
        let heights = rawFormats.compactMap { $0.height }.filter { $0 > 0 }
        let maxHeight = heights.max() ?? 0
        
        let standardResolutions = [
            (2160, "4K"),
            (1440, "1440p"),
            (1080, "1080p"),
            (720, "720p"),
            (480, "480p"),
            (360, "360p")
        ]
        
        var addedResolutions = Set<Int>()
        
        for (res, name) in standardResolutions {
            if maxHeight >= res - 100 && !addedResolutions.contains(res) { // -100 for some leeway (e.g., 1000 instead of 1080)
                options.append(CleanFormatOption(
                    name: "Video \(name)",
                    formatSelector: "bestvideo[height<=\(res)]+bestaudio/best",
                    isAudioOnly: false,
                    symbol: "play.tv.fill"
                ))
                addedResolutions.insert(res)
            }
        }
        
        if options.isEmpty && maxHeight > 0 {
            // Fallback if no standard resolution matched
            options.append(CleanFormatOption(
                name: "Best Video",
                formatSelector: "bestvideo+bestaudio/best",
                isAudioOnly: false,
                symbol: "play.tv.fill"
            ))
        } else if options.isEmpty {
            // If we really don't have height info, just offer best
            options.append(CleanFormatOption(
                name: "Default Video",
                formatSelector: "best",
                isAudioOnly: false,
                symbol: "play.tv.fill"
            ))
        }
        
        // Add Audio options
        options.append(CleanFormatOption(
            name: "Audio MP3",
            formatSelector: "bestaudio/best", // Actual extraction to MP3 needs ffmpeg, which we have. We will handle the conversion flags later in the download engine.
            isAudioOnly: true,
            symbol: "music.note"
        ))
        
        options.append(CleanFormatOption(
            name: "Audio M4A",
            formatSelector: "bestaudio[ext=m4a]/bestaudio/best",
            isAudioOnly: true,
            symbol: "waveform"
        ))
        
        return options
    }
}

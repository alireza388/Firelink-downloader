import Foundation
import Combine

enum AddonState: Equatable, Sendable {
    case notInstalled
    case installed(version: String)
    case failed(error: String)
}

enum AddonType: String, CaseIterable, Sendable {
    case ytDlp = "yt-dlp"
    case ffmpeg

    var binaryName: String {
        switch self {
        case .ytDlp: return "yt-dlp"
        case .ffmpeg: return "ffmpeg"
        }
    }
}

@MainActor
final class MediaEngineManager: ObservableObject {
    static let shared = MediaEngineManager()

    @Published var ytDlpState: AddonState = .notInstalled
    @Published var ffmpegState: AddonState = .notInstalled

    private init() {
        checkLocalInstallation()
        Task.detached { [weak self] in
            await self?.prewarmYtDlp()
        }
    }

    nonisolated private func prewarmYtDlp() async {
        guard let path = await MainActor.run(body: { binaryPath(for: .ytDlp) }) else { return }
        let process = Process()
        process.executableURL = path
        process.arguments = ["--version"]
        process.standardOutput = nil
        process.standardError = nil
        try? process.run()
        process.waitUntilExit()
    }

    func binaryPath(for addon: AddonType) -> URL? {
        if let bundled = Bundle.main.url(forResource: addon.binaryName, withExtension: nil),
           FileManager.default.isExecutableFile(atPath: bundled.path) {
            return bundled
        }
        
        // Prevent fatalError crash: avoid accessing Bundle.module if running in a packaged app.
        if Bundle.main.bundleURL.pathExtension.lowercased() != "app" {
            #if SWIFT_PACKAGE
            if let bundled = Bundle.module.url(forResource: addon.binaryName, withExtension: nil),
               FileManager.default.isExecutableFile(atPath: bundled.path) {
                return bundled
            }
            #endif
        }
        return nil
    }

    func checkLocalInstallation() {
        for addon in AddonType.allCases {
            if binaryPath(for: addon) != nil {
                setState(for: addon, to: .installed(version: "Bundled"))
            } else {
                setState(for: addon, to: .notInstalled)
            }
        }
    }

    func ensureAvailable(addons requiredAddons: Set<AddonType>) async throws {
        checkLocalInstallation()
        let missingAddons = requiredAddons.filter { addon in
            switch state(for: addon) {
            case .installed:
                return false
            case .notInstalled, .failed:
                return true
            }
        }

        guard !missingAddons.isEmpty else { return }

        for missing in missingAddons {
            setState(for: missing, to: .failed(error: "Bundled executable missing"))
        }

        throw NSError(domain: "MediaEngineErrorDomain", code: 1, userInfo: [NSLocalizedDescriptionKey: "One or more required media engines are missing from the app bundle. Reinstall Firelink or rebuild the app bundle."])
    }

    private func state(for addon: AddonType) -> AddonState {
        switch addon {
        case .ytDlp: return ytDlpState
        case .ffmpeg: return ffmpegState
        }
    }

    private func setState(for addon: AddonType, to state: AddonState) {
        switch addon {
        case .ytDlp: ytDlpState = state
        case .ffmpeg: ffmpegState = state
        }
    }
}

import Foundation

struct AddonConfig: Codable, Equatable, Sendable {
    let version: String
    let macArm64: URL?
    let macX64: URL?
    
    enum CodingKeys: String, CodingKey {
        case version
        case macArm64 = "mac-arm64"
        case macX64 = "mac-x64"
    }
    
    /// Returns the appropriate download URL for the current system architecture
    var currentArchURL: URL? {
        #if arch(arm64)
        return macArm64
        #elseif arch(x86_64)
        return macX64
        #else
        return nil
        #endif
    }
}

struct GatekeeperConfig: Codable, Equatable, Sendable {
    let ytDlp: AddonConfig?
    let ffmpeg: AddonConfig?
    
    enum CodingKeys: String, CodingKey {
        case ytDlp = "yt-dlp"
        case ffmpeg
    }
}

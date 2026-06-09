import Foundation

struct GitHubReleaseCheckService: @unchecked Sendable {
    private let owner: String
    private let repository: String
    private let fetch: @Sendable (URLRequest) async throws -> (Data, URLResponse)

    init(
        owner: String = "nimbold",
        repository: String = "Firelink",
        fetch: @escaping @Sendable (URLRequest) async throws -> (Data, URLResponse) = { request in
            try await URLSession.shared.data(for: request)
        }
    ) {
        self.owner = owner
        self.repository = repository
        self.fetch = fetch
    }

    func checkForUpdate(currentVersion: String) async throws -> ReleaseCheckOutcome {
        guard let current = AppVersion(currentVersion) else {
            throw ReleaseCheckFailure.invalidCurrentVersion(currentVersion)
        }

        let release = try await latestStableRelease()
        guard let latest = AppVersion(release.tagName) else {
            throw ReleaseCheckFailure.invalidReleaseVersion(release.tagName)
        }

        let update = AvailableReleaseUpdate(
            version: latest.description,
            tagName: release.tagName,
            title: release.name?.isEmpty == false ? release.name! : release.tagName,
            releaseNotes: release.body?.isEmpty == false ? release.body! : "No release notes were provided for this version.",
            releaseURL: release.htmlURL,
            publishedAt: release.publishedAt
        )

        if latest > current {
            return .updateAvailable(update)
        }

        return .upToDate(latestVersion: latest.description, localVersion: current.description)
    }

    private func latestStableRelease() async throws -> GitHubRelease {
        guard let url = URL(string: "https://api.github.com/repos/\(owner)/\(repository)/releases?per_page=30") else {
            throw ReleaseCheckFailure.invalidReleaseURL
        }

        var request = URLRequest(url: url)
        request.setValue("application/vnd.github+json", forHTTPHeaderField: "Accept")
        request.setValue("Firelink", forHTTPHeaderField: "User-Agent")

        let (data, response) = try await fetch(request)

        guard let httpResponse = response as? HTTPURLResponse else {
            throw ReleaseCheckFailure.invalidResponse
        }

        guard (200..<300).contains(httpResponse.statusCode) else {
            throw ReleaseCheckFailure.httpStatus(httpResponse.statusCode)
        }

        let releases = try JSONDecoder.githubReleaseDecoder.decode([GitHubRelease].self, from: data)
        let stableReleases = releases.filter { !$0.draft && !$0.prerelease }
        guard !stableReleases.isEmpty else {
            throw ReleaseCheckFailure.noStableRelease
        }

        let versionedReleases = stableReleases.compactMap { release -> (release: GitHubRelease, version: AppVersion)? in
            guard let version = AppVersion(release.tagName) else { return nil }
            return (release, version)
        }

        guard let latest = versionedReleases.max(by: { $0.version < $1.version }) else {
            throw ReleaseCheckFailure.invalidReleaseVersion(stableReleases[0].tagName)
        }

        return latest.release
    }
}

struct GitHubRelease: Decodable, Equatable {
    let tagName: String
    let name: String?
    let body: String?
    let htmlURL: URL
    let draft: Bool
    let prerelease: Bool
    let publishedAt: Date?

    private enum CodingKeys: String, CodingKey {
        case tagName = "tag_name"
        case name
        case body
        case htmlURL = "html_url"
        case draft
        case prerelease
        case publishedAt = "published_at"
    }
}

struct AvailableReleaseUpdate: Equatable, Identifiable {
    var id: String { tagName }
    let version: String
    let tagName: String
    let title: String
    let releaseNotes: String
    let releaseURL: URL
    let publishedAt: Date?
}

enum ReleaseCheckOutcome: Equatable {
    case updateAvailable(AvailableReleaseUpdate)
    case upToDate(latestVersion: String, localVersion: String)
}

enum ReleaseUpdateState: Equatable {
    case idle
    case checking
    case updateAvailable(AvailableReleaseUpdate)
    case upToDate(latestVersion: String, localVersion: String)
    case failed(message: String, recovery: String)
}

enum ReleaseCheckFailure: Error, Equatable {
    case invalidReleaseURL
    case invalidResponse
    case httpStatus(Int)
    case noStableRelease
    case invalidCurrentVersion(String)
    case invalidReleaseVersion(String)
}

@MainActor
final class ReleaseUpdateChecker: ObservableObject {
    @Published private(set) var state: ReleaseUpdateState = .idle
    @Published var automaticallyChecksForUpdates: Bool {
        didSet {
            UserDefaults.standard.set(automaticallyChecksForUpdates, forKey: Self.automaticChecksKey)
        }
    }

    private let service: GitHubReleaseCheckService
    private let bundle: Bundle
    private var automaticCheckTask: Task<Void, Never>?

    private static let automaticChecksKey = "AutomaticallyCheckForReleaseUpdates"
    private static let lastAutomaticCheckKey = "LastReleaseUpdateCheckDate"
    private static let automaticCheckInterval: TimeInterval = 24 * 60 * 60

    init(service: GitHubReleaseCheckService = GitHubReleaseCheckService(), bundle: Bundle = .main) {
        self.service = service
        self.bundle = bundle

        if UserDefaults.standard.object(forKey: Self.automaticChecksKey) == nil {
            self.automaticallyChecksForUpdates = true
        } else {
            self.automaticallyChecksForUpdates = UserDefaults.standard.bool(forKey: Self.automaticChecksKey)
        }
    }

    var currentVersion: String {
        bundle.infoDictionary?["CFBundleShortVersionString"] as? String ?? "0.1.0"
    }

    func checkForUpdates() {
        guard state != .checking else { return }
        state = .checking

        Task {
            do {
                let outcome = try await service.checkForUpdate(currentVersion: currentVersion)
                UserDefaults.standard.set(Date(), forKey: Self.lastAutomaticCheckKey)
                apply(outcome)
            } catch {
                state = Self.failedState(for: error)
            }
        }
    }

    func checkAutomaticallyIfNeeded() {
        guard automaticallyChecksForUpdates else { return }
        guard state == .idle else { return }

        if let lastCheck = UserDefaults.standard.object(forKey: Self.lastAutomaticCheckKey) as? Date,
           Date().timeIntervalSince(lastCheck) < Self.automaticCheckInterval {
            return
        }

        automaticCheckTask?.cancel()
        automaticCheckTask = Task { [weak self] in
            self?.checkForUpdates()
        }
    }

    private func apply(_ outcome: ReleaseCheckOutcome) {
        switch outcome {
        case .updateAvailable(let update):
            state = .updateAvailable(update)
        case .upToDate(let latestVersion, let localVersion):
            state = .upToDate(latestVersion: latestVersion, localVersion: localVersion)
        }
    }

    private static func failedState(for error: Error) -> ReleaseUpdateState {
        if let urlError = error as? URLError {
            switch urlError.code {
            case .notConnectedToInternet, .networkConnectionLost, .cannotFindHost, .cannotConnectToHost, .timedOut:
                return .failed(
                    message: "Couldn't reach GitHub.",
                    recovery: "Check your internet connection, then try again."
                )
            default:
                return .failed(
                    message: "The update check couldn't finish.",
                    recovery: urlError.localizedDescription
                )
            }
        }

        if let failure = error as? ReleaseCheckFailure {
            switch failure {
            case .httpStatus(let statusCode):
                return .failed(
                    message: "GitHub returned HTTP \(statusCode).",
                    recovery: "Try again in a moment, or open the releases page manually."
                )
            case .noStableRelease:
                return .failed(
                    message: "No stable release was found.",
                    recovery: "Open GitHub Releases to check the project manually."
                )
            case .invalidCurrentVersion(let version):
                return .failed(
                    message: "Firelink's current version could not be read.",
                    recovery: "The app reported version \(version)."
                )
            case .invalidReleaseVersion(let tag):
                return .failed(
                    message: "The latest release tag could not be compared.",
                    recovery: "GitHub reported \(tag)."
                )
            case .invalidReleaseURL, .invalidResponse:
                return .failed(
                    message: "The GitHub response could not be read.",
                    recovery: "Try again in a moment."
                )
            }
        }

        return .failed(
            message: "The update check failed.",
            recovery: error.localizedDescription
        )
    }
}

struct AppVersion: Comparable, CustomStringConvertible {
    private let components: [Int]

    init?(_ rawValue: String) {
        var value = rawValue.trimmingCharacters(in: .whitespacesAndNewlines)
        if value.hasPrefix("v") || value.hasPrefix("V") {
            value.removeFirst()
        }

        let prefix = value.prefix { character in
            character.isNumber || character == "."
        }

        let version = String(prefix).trimmingCharacters(in: CharacterSet(charactersIn: "."))
        guard !version.isEmpty else { return nil }

        let components = version.split(separator: ".").compactMap { Int($0) }
        guard !components.isEmpty, components.count == version.split(separator: ".").count else {
            return nil
        }

        self.components = components
    }

    var description: String {
        components.map(String.init).joined(separator: ".")
    }

    static func < (lhs: AppVersion, rhs: AppVersion) -> Bool {
        let count = max(lhs.components.count, rhs.components.count)

        for index in 0..<count {
            let left = index < lhs.components.count ? lhs.components[index] : 0
            let right = index < rhs.components.count ? rhs.components[index] : 0

            if left != right {
                return left < right
            }
        }

        return false
    }
}

private extension JSONDecoder {
    static var githubReleaseDecoder: JSONDecoder {
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        return decoder
    }
}

import Foundation
import AppKit
import Sparkle

class InlineUpdateUserDriver: NSObject, SPUUserDriver {
    weak var updater: SparkleUpdater?
    
    init(updater: SparkleUpdater) {
        self.updater = updater
    }
    
    func show(_ request: SPUUpdatePermissionRequest, reply: @escaping (SUUpdatePermissionResponse) -> Void) {
        reply(SUUpdatePermissionResponse(automaticUpdateChecks: true, sendSystemProfile: false))
    }
    
    func showUserInitiatedUpdateCheck(cancellation: @escaping () -> Void) {
        DispatchQueue.main.async {
            self.updater?.resetState()
            self.updater?.isChecking = true
            self.updater?.updateStatus = "Checking for updates..."
            self.updater?.cancellation = cancellation
        }
    }
    
    func showUpdateFound(with appcastItem: SUAppcastItem, state: SPUUserUpdateState, reply: @escaping (SPUUserUpdateChoice) -> Void) {
        DispatchQueue.main.async {
            self.updater?.isChecking = false
            self.updater?.foundUpdateItem = appcastItem
            self.updater?.updateStatus = "Update available: Version \(appcastItem.displayVersionString)"
            self.updater?.updateChoiceReply = reply
        }
    }
    
    func showUpdateReleaseNotes(with downloadData: SPUDownloadData) {
        DispatchQueue.global(qos: .userInitiated).async {
            if let htmlString = String(data: downloadData.data, encoding: .utf8) {
                let parsedText = self.fastHTMLToMarkdown(htmlString)
                DispatchQueue.main.async {
                    self.updater?.releaseNotes = parsedText
                }
            }
        }
    }
    
    nonisolated private func fastHTMLToMarkdown(_ html: String) -> String {
        var text = html
        text = text.replacingOccurrences(of: "<br>", with: "\n", options: .caseInsensitive)
        text = text.replacingOccurrences(of: "<br/>", with: "\n", options: .caseInsensitive)
        text = text.replacingOccurrences(of: "<br />", with: "\n", options: .caseInsensitive)
        text = text.replacingOccurrences(of: "</p>", with: "\n\n", options: .caseInsensitive)
        text = text.replacingOccurrences(of: "<li>", with: "- ", options: .caseInsensitive)
        text = text.replacingOccurrences(of: "</li>", with: "\n", options: .caseInsensitive)
        text = text.replacingOccurrences(of: "<h1>", with: "# ", options: .caseInsensitive)
        text = text.replacingOccurrences(of: "</h1>", with: "\n\n", options: .caseInsensitive)
        text = text.replacingOccurrences(of: "<h2>", with: "## ", options: .caseInsensitive)
        text = text.replacingOccurrences(of: "</h2>", with: "\n\n", options: .caseInsensitive)
        text = text.replacingOccurrences(of: "<h3>", with: "### ", options: .caseInsensitive)
        text = text.replacingOccurrences(of: "</h3>", with: "\n\n", options: .caseInsensitive)
        text = text.replacingOccurrences(of: "<b>", with: "**", options: .caseInsensitive)
        text = text.replacingOccurrences(of: "</b>", with: "**", options: .caseInsensitive)
        text = text.replacingOccurrences(of: "<strong>", with: "**", options: .caseInsensitive)
        text = text.replacingOccurrences(of: "</strong>", with: "**", options: .caseInsensitive)
        
        if let regex = try? NSRegularExpression(pattern: "<[^>]+>", options: .caseInsensitive) {
            let range = NSRange(location: 0, length: text.utf16.count)
            text = regex.stringByReplacingMatches(in: text, options: [], range: range, withTemplate: "")
        }
        
        text = text.replacingOccurrences(of: "&nbsp;", with: " ")
        text = text.replacingOccurrences(of: "&amp;", with: "&")
        text = text.replacingOccurrences(of: "&lt;", with: "<")
        text = text.replacingOccurrences(of: "&gt;", with: ">")
        text = text.replacingOccurrences(of: "&quot;", with: "\"")
        text = text.replacingOccurrences(of: "&#39;", with: "'")
        
        if let regex = try? NSRegularExpression(pattern: "\\n{3,}", options: []) {
            let range = NSRange(location: 0, length: text.utf16.count)
            text = regex.stringByReplacingMatches(in: text, options: [], range: range, withTemplate: "\n\n")
        }
        
        return text.trimmingCharacters(in: .whitespacesAndNewlines)
    }
    
    func showUpdateReleaseNotesFailedToDownloadWithError(_ error: Error) {
    }
    
    func showUpdateNotFoundWithError(_ error: Error, acknowledgement: @escaping () -> Void) {
        DispatchQueue.main.async {
            self.updater?.isChecking = false
            let nsError = error as NSError
            if nsError.domain == SUSparkleErrorDomain && nsError.code == 1001 {
                self.updater?.updateStatus = "You're up to date!"
            } else {
                self.updater?.updateStatus = "Update check failed: \(error.localizedDescription)"
            }
            acknowledgement()
        }
    }
    
    func showUpdaterError(_ error: Error, acknowledgement: @escaping () -> Void) {
        DispatchQueue.main.async {
            self.updater?.isChecking = false
            self.updater?.updateStatus = "Updater error: \(error.localizedDescription)"
            acknowledgement()
        }
    }
    
    func showDownloadInitiated(cancellation: @escaping () -> Void) {
        DispatchQueue.main.async {
            self.updater?.isDownloading = true
            self.updater?.downloadProgress = 0.0
            self.updater?.cancellation = cancellation
            self.updater?.updateStatus = "Downloading update..."
        }
    }
    
    func showDownloadDidReceiveExpectedContentLength(_ expectedContentLength: UInt64) {
        DispatchQueue.main.async {
            self.updater?.expectedContentLength = expectedContentLength
            self.updater?.receivedContentLength = 0
        }
    }
    
    func showDownloadDidReceiveData(ofLength length: UInt64) {
        DispatchQueue.main.async {
            if let updater = self.updater {
                updater.receivedContentLength += length
                if updater.expectedContentLength > 0 {
                    updater.downloadProgress = Double(updater.receivedContentLength) / Double(updater.expectedContentLength)
                }
            }
        }
    }
    
    func showDownloadDidStartExtractingUpdate() {
        DispatchQueue.main.async {
            self.updater?.isDownloading = false
            self.updater?.isExtracting = true
            self.updater?.updateStatus = "Extracting update..."
            self.updater?.downloadProgress = 1.0
        }
    }
    
    func showExtractionReceivedProgress(_ progress: Double) {
        DispatchQueue.main.async {
            self.updater?.extractionProgress = progress
        }
    }
    
    func showReady(toInstallAndRelaunch reply: @escaping (SPUUserUpdateChoice) -> Void) {
        DispatchQueue.main.async {
            self.updater?.isExtracting = false
            self.updater?.isReadyToInstall = true
            self.updater?.updateStatus = "Ready to install"
            self.updater?.updateChoiceReply = reply
        }
    }
    
    func showInstallingUpdate(withApplicationTerminated applicationTerminated: Bool, retryTerminatingApplication: @escaping () -> Void) {
    }
    
    func showUpdateInstalledAndRelaunched(_ relaunched: Bool, acknowledgement: @escaping () -> Void) {
        acknowledgement()
    }
    
    func dismissUpdateInstallation() {
        DispatchQueue.main.async {
            self.updater?.isChecking = false
            self.updater?.isDownloading = false
            self.updater?.isExtracting = false
            self.updater?.isReadyToInstall = false
            self.updater?.downloadProgress = 0.0
            self.updater?.extractionProgress = 0.0
            self.updater?.foundUpdateItem = nil
            self.updater?.releaseNotes = nil
            // Do not clear updateStatus here so success/error messages remain visible.
        }
    }
    
    func showUpdateInFocus() {
    }
}

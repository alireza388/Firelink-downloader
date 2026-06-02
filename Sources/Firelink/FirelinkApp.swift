import SwiftUI

@main
struct FirelinkApp: App {
    @StateObject private var settings: AppSettings
    @StateObject private var controller: DownloadController

    init() {
        let settings = AppSettings()
        _settings = StateObject(wrappedValue: settings)
        _controller = StateObject(wrappedValue: DownloadController(settings: settings))
    }

    var body: some Scene {
        WindowGroup {
            ContentView()
                .environmentObject(controller)
                .environmentObject(settings)
                .frame(minWidth: 1180, idealWidth: 1280, minHeight: 720, idealHeight: 760)
        }
        .windowStyle(.titleBar)

        WindowGroup("Add Downloads", id: "add-downloads") {
            AddDownloadsView()
                .environmentObject(controller)
                .environmentObject(settings)
        }
        .windowResizability(.contentSize)

        WindowGroup("Download Properties", for: UUID.self) { $downloadID in
            if let downloadID {
                DownloadPropertiesWindow(downloadID: downloadID)
                    .environmentObject(controller)
                    .environmentObject(settings)
            } else {
                ContentUnavailableView("Download Not Found", systemImage: "questionmark.circle")
            }
        }
        .windowResizability(.contentSize)

        .commands {
            CommandGroup(after: .newItem) {
                Button("Start Queue") {
                    controller.startQueue()
                }
                .keyboardShortcut("r", modifiers: [.command])
            }
        }
    }
}

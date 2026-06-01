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
                .frame(minWidth: 980, minHeight: 640)
        }
        .windowStyle(.titleBar)
        .commands {
            CommandGroup(after: .newItem) {
                Button("Start Queue") {
                    controller.startQueue()
                }
                .keyboardShortcut("r", modifiers: [.command])
            }
        }

        Settings {
            SettingsView()
                .environmentObject(settings)
        }
    }
}

import SwiftUI

@main
struct FirelinkApp: App {
    @StateObject private var settings: AppSettings
    @StateObject private var controller: DownloadController
    @StateObject private var schedulerController: SchedulerController

    init() {
        let settings = AppSettings()
        let controller = DownloadController(settings: settings)
        _settings = StateObject(wrappedValue: settings)
        _controller = StateObject(wrappedValue: controller)
        _schedulerController = StateObject(wrappedValue: SchedulerController(downloadController: controller))
    }

    var body: some Scene {
        WindowGroup(id: "main") {
            ContentView()
                .environmentObject(controller)
                .environmentObject(settings)
                .environmentObject(schedulerController)
                .modifier(AppThemeModifier(theme: settings.appTheme))
                .frame(minWidth: 1180, idealWidth: 1280, minHeight: 720, idealHeight: 760)
        }
        .windowStyle(.titleBar)

        WindowGroup("Add Downloads", id: "add-downloads") {
            AddDownloadsView()
                .environmentObject(controller)
                .environmentObject(settings)
                .modifier(AppThemeModifier(theme: settings.appTheme))
        }
        .windowResizability(.contentSize)

        WindowGroup("Download Properties", for: UUID.self) { $downloadID in
            if let downloadID {
                DownloadPropertiesWindow(downloadID: downloadID)
                    .environmentObject(controller)
                    .environmentObject(settings)
                    .modifier(AppThemeModifier(theme: settings.appTheme))
            } else {
                ContentUnavailableView("Download Not Found", systemImage: "questionmark.circle")
                    .modifier(AppThemeModifier(theme: settings.appTheme))
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

        MenuBarExtra("Firelink", systemImage: "arrow.down.circle") {
            TrayMenuView()
                .environmentObject(controller)
        }
    }
}

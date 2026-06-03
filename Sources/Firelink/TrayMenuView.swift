import SwiftUI

struct TrayMenuView: View {
    @Environment(\.openWindow) private var openWindow
    @EnvironmentObject private var controller: DownloadController

    var body: some View {
        Button("Show main window") {
            openWindow(id: "main")
            NSApp.activate(ignoringOtherApps: true)
        }

        Button("Add downloads") {
            openWindow(id: "add-downloads")
            NSApp.activate(ignoringOtherApps: true)
        }

        Menu("Start queue") {
            ForEach(controller.queues) { queue in
                Button(queue.name) {
                    controller.startQueue(queueID: queue.id)
                }
            }
        }

        Button("Stop downloads") {
            for download in controller.downloads where download.status == .downloading {
                controller.pause(download)
            }
        }

        Divider()

        Button("Exit") {
            NSApplication.shared.terminate(nil)
        }
    }
}

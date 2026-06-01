import SwiftUI

struct StatusBar: View {
    @EnvironmentObject private var controller: DownloadController

    var body: some View {
        HStack {
            Text(controller.engineMessage.isEmpty ? "Ready" : controller.engineMessage)
                .foregroundStyle(controller.hasAria2 ? Color.secondary : Color.orange)
                .lineLimit(1)
            Spacer()
            Text("\(controller.activeCount) active")
            Text("\(controller.queuedCount) queued")
            Text("\(controller.completedCount) done")
        }
        .font(.caption)
        .padding(.horizontal, 14)
        .padding(.vertical, 8)
        .background(.bar)
    }
}

import SwiftUI

struct DuplicateDownloadItem: Identifiable, Equatable {
    let id: UUID
    var pendingItem: PendingDownload
    var resolution: DuplicateResolutionAction = .rename
    let reason: DuplicateReason
    
    enum DuplicateReason: Equatable {
        case existingURL(String)
        case existingFile(String)
    }
}

enum DuplicateResolutionAction: String, CaseIterable, Identifiable {
    case rename = "Rename"
    case replace = "Replace"
    case skip = "Skip"
    var id: String { rawValue }
}

struct DuplicateResolutionView: View {
    @Binding var conflicts: [DuplicateDownloadItem]
    let onConfirm: () -> Void
    let onCancel: () -> Void
    
    var body: some View {
        VStack(spacing: 0) {
            VStack(alignment: .leading, spacing: 8) {
                Text("Duplicate Downloads Detected")
                    .font(.headline)
                Text("Some of the downloads you are adding already exist in the queue or on disk. Please choose how to resolve these conflicts.")
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
            }
            .padding(16)
            .frame(maxWidth: .infinity, alignment: .leading)
            
            Divider()
            
            List($conflicts) { $conflict in
                HStack {
                    VStack(alignment: .leading, spacing: 2) {
                        Text(conflict.pendingItem.fileName)
                            .lineLimit(1)
                            .font(.body.weight(.medium))
                        Text(reasonText(for: conflict.reason))
                            .font(.caption)
                            .foregroundStyle(.orange)
                    }
                    
                    Spacer()
                    
                    Picker("", selection: $conflict.resolution) {
                        ForEach(DuplicateResolutionAction.allCases) { action in
                            Text(action.rawValue).tag(action)
                        }
                    }
                    .labelsHidden()
                    .frame(width: 100)
                }
                .padding(.vertical, 4)
            }
            .frame(minHeight: 200)
            
            Divider()
            
            HStack {
                Button("Cancel", action: onCancel)
                    .keyboardShortcut(.cancelAction)
                Spacer()
                Button("Continue", action: onConfirm)
                    .buttonStyle(.borderedProminent)
                    .keyboardShortcut(.defaultAction)
            }
            .padding(16)
        }
        .frame(width: 500)
    }
    
    private func reasonText(for reason: DuplicateDownloadItem.DuplicateReason) -> String {
        switch reason {
        case .existingURL(let msg): return msg
        case .existingFile(let msg): return msg
        }
    }
}

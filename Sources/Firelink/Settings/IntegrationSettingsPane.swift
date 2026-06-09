import AppKit
import SwiftUI

struct IntegrationSettingsPane: View {
    @EnvironmentObject private var controller: DownloadController
    @EnvironmentObject private var settings: AppSettings
    @State private var showToast = false

    var body: some View {
        ScrollView {
            VStack(spacing: 24) {
                // Header
                HStack(alignment: .center, spacing: 16) {
                    Image(systemName: "puzzlepiece.extension.fill")
                        .resizable()
                        .aspectRatio(contentMode: .fit)
                        .frame(width: 48, height: 48)
                        .foregroundStyle(Color(nsColor: NSColor(red: 1.0, green: 0.44, blue: 0.22, alpha: 1.0)))
                        .accessibilityHidden(true)

                    VStack(alignment: .leading, spacing: 4) {
                        Text("Connect Browser Extension")
                            .font(.title.weight(.bold))
                        Text("Capture downloads directly from your browser in three easy steps.")
                            .foregroundStyle(.secondary)
                            .font(.body)
                    }
                    Spacer()
                }
                .padding(.bottom, 8)

                // Step 1: Copy Token
                StepCardView(
                    stepNumber: 1,
                    title: "Copy Pairing Token",
                    description: "This secure token authorizes your browser extension.",
                    icon: "doc.on.clipboard.fill",
                    iconColor: .blue,
                    actionText: "Copy Token"
                ) {
                    NSPasteboard.general.clearContents()
                    NSPasteboard.general.setString(settings.extensionPairingToken, forType: .string)
                    withAnimation {
                        showToast = true
                    }
                }

                // Step 2: Open Browser
                StepCardView(
                    stepNumber: 2,
                    title: "Open Firefox",
                    description: "Launch your browser. If you haven't installed the extension yet, do that first.",
                    icon: "safari.fill",
                    iconColor: .orange,
                    actionText: "Open Firefox"
                ) {
                    if let url = URL(string: "https://addons.mozilla.org/en-US/firefox/addon/firelink-companion/") {
                        NSWorkspace.shared.open(url)
                    }
                }

                // Step 3: Paste and Save
                StepCardView(
                    stepNumber: 3,
                    title: "Paste & Connect",
                    description: "Click the Firelink icon in your browser's toolbar and paste the token into the App Pairing Token field.",
                    icon: "arrow.down.doc.fill",
                    iconColor: .green,
                    actionText: nil,
                    action: nil
                )
                
                Divider()
                
                // Diagnostics
                HStack {
                    Text("Diagnostics:")
                        .foregroundStyle(.secondary)
                    Spacer()
                    if let port = controller.extensionServerPort {
                        Label("Listening on 127.0.0.1:\(port)", systemImage: "checkmark.seal.fill")
                            .foregroundStyle(.green)
                    } else {
                        Label("Not listening", systemImage: "exclamationmark.triangle.fill")
                            .foregroundStyle(.orange)
                    }
                }
                .font(.footnote)
                .padding(.top, 8)

            }
            .padding(32)
        }
        .toast(isShowing: $showToast, message: "Token copied to clipboard!")
        .background(Color(NSColor.windowBackgroundColor))
    }
}

struct StepCardView: View {
    let stepNumber: Int
    let title: String
    let description: String
    let icon: String
    let iconColor: Color
    let actionText: String?
    let action: (() -> Void)?

    var body: some View {
        HStack(spacing: 16) {
            // Step Number Badge
            ZStack {
                Circle()
                    .fill(Color(nsColor: .controlBackgroundColor))
                    .frame(width: 32, height: 32)
                    .shadow(color: .black.opacity(0.1), radius: 2, y: 1)
                
                Text("\(stepNumber)")
                    .font(.system(.headline, design: .rounded).weight(.bold))
                    .foregroundStyle(.primary)
            }
            
            // Icon
            ZStack {
                RoundedRectangle(cornerRadius: 12)
                    .fill(iconColor.opacity(0.15))
                    .frame(width: 48, height: 48)
                
                Image(systemName: icon)
                    .font(.system(size: 24))
                    .foregroundStyle(iconColor)
            }
            
            // Text Content
            VStack(alignment: .leading, spacing: 4) {
                Text(title)
                    .font(.headline)
                Text(description)
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }
            
            Spacer()
            
            // Action Button
            if let actionText = actionText, let action = action {
                Button(action: action) {
                    Text(actionText)
                        .font(.subheadline.weight(.medium))
                        .padding(.horizontal, 16)
                        .padding(.vertical, 8)
                        .background(Color.accentColor)
                        .foregroundColor(.white)
                        .cornerRadius(8)
                }
                .buttonStyle(.plain)
            }
        }
        .padding(16)
        .background(
            RoundedRectangle(cornerRadius: 16)
                .fill(Color(nsColor: .controlBackgroundColor))
                .shadow(color: .black.opacity(0.05), radius: 8, y: 2)
        )
        .overlay(
            RoundedRectangle(cornerRadius: 16)
                .strokeBorder(Color(nsColor: .separatorColor).opacity(0.5), lineWidth: 1)
        )
    }
}

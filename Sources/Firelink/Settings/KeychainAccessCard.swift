import SwiftUI

struct KeychainAccessCard: View {
    @EnvironmentObject private var settings: AppSettings
    
    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            HStack(spacing: 16) {
                ZStack {
                    RoundedRectangle(cornerRadius: 12)
                        .fill(settings.isKeychainAccessGranted ? Color.green.opacity(0.15) : Color.blue.opacity(0.15))
                        .frame(width: 48, height: 48)
                    
                    Image(systemName: settings.isKeychainAccessGranted ? "lock.open.fill" : "lock.fill")
                        .font(.system(size: 24))
                        .foregroundStyle(settings.isKeychainAccessGranted ? .green : .blue)
                }
                
                VStack(alignment: .leading, spacing: 4) {
                    Text("Keychain Access")
                        .font(.headline)
                    Text("Firelink needs Keychain access to securely store your browser extension pairing token and site login passwords.")
                        .font(.subheadline)
                        .foregroundStyle(.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                }
                Spacer()
            }
            
            HStack {
                Spacer()
                if settings.isKeychainAccessGranted {
                    Label("Access Granted", systemImage: "checkmark.circle.fill")
                        .foregroundStyle(.green)
                        .font(.subheadline.weight(.medium))
                        .padding(.trailing, 8)
                    
                    Button(role: .destructive) {
                        settings.revokeKeychainAccess()
                    } label: {
                        Text("Revoke Access")
                    }
                } else {
                    Button {
                        settings.grantKeychainAccess()
                    } label: {
                        Text("Grant Access")
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

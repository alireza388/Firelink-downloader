import SwiftUI

struct KeychainAccessCard: View {
    @EnvironmentObject private var settings: AppSettings
    
    var body: some View {
        HStack(spacing: settings.isKeychainAccessGranted ? 12 : 16) {
            ZStack {
                RoundedRectangle(cornerRadius: settings.isKeychainAccessGranted ? 6 : 10)
                    .fill(settings.isKeychainAccessGranted ? Color.green.opacity(0.15) : Color.blue.opacity(0.15))
                    .frame(width: settings.isKeychainAccessGranted ? 28 : 36, height: settings.isKeychainAccessGranted ? 28 : 36)
                
                Image(systemName: settings.isKeychainAccessGranted ? "lock.open.fill" : "lock.fill")
                    .font(.system(size: settings.isKeychainAccessGranted ? 14 : 18))
                    .foregroundStyle(settings.isKeychainAccessGranted ? .green : .blue)
            }
            
            VStack(alignment: .leading, spacing: 2) {
                Text(settings.isKeychainAccessGranted ? "Keychain Access Granted" : "Keychain Access")
                    .font(settings.isKeychainAccessGranted ? .subheadline.weight(.medium) : .headline)
                    .foregroundStyle(settings.isKeychainAccessGranted ? .green : .primary)
                
                if !settings.isKeychainAccessGranted {
                    Text("Firelink needs Keychain access to securely store your browser extension pairing token.")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
            
            Spacer(minLength: settings.isKeychainAccessGranted ? 8 : 16)
            
            if settings.isKeychainAccessGranted {
                Button(role: .destructive) {
                    settings.revokeKeychainAccess()
                } label: {
                    Text("Revoke")
                }
                .controlSize(.small)
            } else {
                Button {
                    settings.grantKeychainAccess()
                } label: {
                    Text("Grant Access")
                        .font(.subheadline.weight(.medium))
                        .padding(.horizontal, 12)
                        .padding(.vertical, 6)
                        .background(Color.accentColor)
                        .foregroundColor(.white)
                        .cornerRadius(6)
                }
                .buttonStyle(.plain)
            }
        }
        .padding(settings.isKeychainAccessGranted ? 8 : 12)
        .background(
            RoundedRectangle(cornerRadius: settings.isKeychainAccessGranted ? 8 : 12)
                .fill(Color(nsColor: .controlBackgroundColor))
                .shadow(color: .black.opacity(0.05), radius: 4, y: 1)
        )
        .overlay(
            RoundedRectangle(cornerRadius: settings.isKeychainAccessGranted ? 8 : 12)
                .strokeBorder(Color(nsColor: .separatorColor).opacity(0.5), lineWidth: 1)
        )
    }
}

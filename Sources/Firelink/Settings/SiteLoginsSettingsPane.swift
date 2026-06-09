import SwiftUI

struct SiteLoginsSettingsPane: View {
    @EnvironmentObject private var settings: AppSettings
    @State private var urlPattern = ""
    @State private var username = ""
    @State private var password = ""
    @State private var editingLoginID: UUID?

    var body: some View {
        Form {
            Section {
                KeychainAccessCard()
            }
            .listRowBackground(Color.clear)
            .listRowInsets(EdgeInsets())

            if settings.isKeychainAccessGranted {
                Section(editingLoginID == nil ? "Add Login" : "Edit Login") {
                TextField("URL Pattern (e.g., *.github.com)", text: $urlPattern)
                TextField("Username", text: $username)
                SecureField(editingLoginID == nil ? "Password" : "Password (leave blank to keep current)", text: $password)

                HStack {
                    Text(settings.message)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                    Spacer()
                    if editingLoginID != nil {
                        Button("Cancel Edit") {
                            resetForm()
                        }
                    }
                    Button {
                        settings.saveSiteLogin(
                            id: editingLoginID,
                            urlPattern: urlPattern,
                            username: username,
                            password: password
                        )
                        if settings.message.hasPrefix("Added") || settings.message.hasPrefix("Updated") {
                            resetForm()
                        }
                    } label: {
                        Label(editingLoginID == nil ? "Add Login" : "Save Login", systemImage: editingLoginID == nil ? "plus" : "checkmark")
                    }
                    .buttonStyle(.borderedProminent)
                }
            }

            Section("Saved Logins") {
                if settings.siteLogins.isEmpty {
                    Text("No saved logins.")
                        .foregroundStyle(.secondary)
                } else {
                    List {
                        ForEach(settings.siteLogins) { login in
                            HStack {
                                Image(systemName: "key.horizontal")
                                    .foregroundStyle(.secondary)
                                    .accessibilityHidden(true)
                                Text(login.urlPattern)
                                    .font(.system(.body, design: .monospaced))
                                Spacer()
                                Text(login.username)
                                    .foregroundStyle(.secondary)
                                Button {
                                    edit(login)
                                } label: {
                                    Label("Edit", systemImage: "pencil")
                                }
                                .labelStyle(.iconOnly)
                                .buttonStyle(.borderless)
                            }
                        }
                        .onDelete(perform: settings.deleteSiteLogins)
                    }
                    .frame(minHeight: 180)
                }
                }
            }
        }
        .formStyle(.grouped)
    }

    private func edit(_ login: SiteLogin) {
        editingLoginID = login.id
        urlPattern = login.urlPattern
        username = login.username
        password = ""
    }

    private func resetForm() {
        editingLoginID = nil
        urlPattern = ""
        username = ""
        password = ""
    }
}

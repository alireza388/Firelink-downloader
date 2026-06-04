import SwiftUI

struct SiteLoginsSettingsPane: View {
    @EnvironmentObject private var settings: AppSettings
    @State private var urlPattern = ""
    @State private var username = ""
    @State private var password = ""

    var body: some View {
        Form {
            Section("Add Login") {
                TextField("URL Pattern (e.g., *.github.com)", text: $urlPattern)
                TextField("Username", text: $username)
                SecureField("Password", text: $password)

                HStack {
                    Text(settings.message)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                    Spacer()
                    Button {
                        settings.addSiteLogin(urlPattern: urlPattern, username: username, password: password)
                        if settings.message.hasPrefix("Added") {
                            urlPattern = ""
                            username = ""
                            password = ""
                        }
                    } label: {
                        Label("Add Login", systemImage: "plus")
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
                            }
                        }
                        .onDelete(perform: settings.deleteSiteLogins)
                    }
                    .frame(minHeight: 180)
                }
            }
        }
        .formStyle(.grouped)
    }
}

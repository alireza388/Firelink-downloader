import SwiftUI
import AppKit

struct SiteLoginsSettingsPane: View {
    @EnvironmentObject private var settings: AppSettings
    @State private var searchText = ""
    @State private var selection = Set<SiteLogin.ID>()
    @State private var editingLogin: SiteLogin?
    @State private var showEditor = false

    var filteredLogins: [SiteLogin] {
        if searchText.isEmpty {
            return settings.siteLogins
        } else {
            return settings.siteLogins.filter {
                $0.urlPattern.localizedCaseInsensitiveContains(searchText) ||
                $0.username.localizedCaseInsensitiveContains(searchText)
            }
        }
    }

    var body: some View {
        Form {
            Section {
                KeychainAccessCard()
            }
            .listRowBackground(Color.clear)
            .listRowInsets(EdgeInsets())

            if settings.isKeychainAccessGranted {
                Section {
                    VStack(spacing: 0) {
                        HStack {
                            Image(systemName: "magnifyingglass")
                                .foregroundColor(.secondary)
                            TextField("Search Logins", text: $searchText)
                                .textFieldStyle(.plain)
                            if !searchText.isEmpty {
                                Button {
                                    searchText = ""
                                } label: {
                                    Image(systemName: "xmark.circle.fill")
                                        .foregroundColor(.secondary)
                                }
                                .buttonStyle(.plain)
                            }
                        }
                        .padding(8)
                        .background(Color(NSColor.textBackgroundColor))
                        
                        Divider()

                        if settings.siteLogins.isEmpty {
                            ContentUnavailableView("No saved logins", systemImage: "key")
                                .frame(minHeight: 250)
                        } else if filteredLogins.isEmpty {
                            ContentUnavailableView.search(text: searchText)
                                .frame(minHeight: 250)
                        } else {
                            Table(filteredLogins, selection: $selection) {
                                TableColumn("URL Pattern") { login in
                                    Text(login.urlPattern)
                                        .font(.system(.body, design: .monospaced))
                                        .contextMenu {
                                            contextMenuActions(for: login)
                                        }
                                }
                                TableColumn("Username") { login in
                                    Text(login.username)
                                        .contextMenu {
                                            contextMenuActions(for: login)
                                        }
                                }
                            }
                            .frame(minHeight: 200, idealHeight: 250, maxHeight: 300)
                        }
                        
                        Divider()
                        
                        HStack {
                            Button {
                                editingLogin = nil
                                showEditor = true
                            } label: {
                                Image(systemName: "plus")
                                    .frame(width: 24, height: 24)
                                    .contentShape(Rectangle())
                            }
                            .buttonStyle(.plain)
                            .padding(.horizontal, 4)
                            
                            Divider()
                                .frame(height: 16)
                                
                            Button {
                                deleteSelected()
                            } label: {
                                Image(systemName: "minus")
                                    .frame(width: 24, height: 24)
                                    .contentShape(Rectangle())
                            }
                            .buttonStyle(.plain)
                            .padding(.horizontal, 4)
                            .disabled(selection.isEmpty)
                            
                            Spacer()
                        }
                        .padding(.vertical, 4)
                        .padding(.horizontal, 4)
                        .background(Color(NSColor.windowBackgroundColor))
                    }
                    .background(Color(NSColor.controlBackgroundColor))
                    .cornerRadius(8)
                    .overlay(
                        RoundedRectangle(cornerRadius: 8)
                            .stroke(Color.secondary.opacity(0.2), lineWidth: 1)
                    )
                } header: {
                    Text("Saved Logins")
                }
                .listRowBackground(Color.clear)
                .listRowInsets(EdgeInsets())
            }
        }
        .formStyle(.grouped)
        .sheet(isPresented: $showEditor) {
            LoginEditorSheet(login: editingLogin)
        }
    }
    
    @ViewBuilder
    private func contextMenuActions(for login: SiteLogin) -> some View {
        Button("Edit") {
            editingLogin = login
            showEditor = true
        }
        Divider()
        Button("Copy Username") {
            NSPasteboard.general.clearContents()
            NSPasteboard.general.setString(login.username, forType: .string)
        }
        Button("Copy Password") {
            if let password = KeychainCredentialStore.password(for: login.id) {
                NSPasteboard.general.clearContents()
                NSPasteboard.general.setString(password, forType: .string)
            }
        }
        Divider()
        Button("Delete", role: .destructive) {
            if let index = settings.siteLogins.firstIndex(where: { $0.id == login.id }) {
                settings.deleteSiteLogins(at: IndexSet(integer: index))
            }
            selection.remove(login.id)
        }
    }
    
    private func deleteSelected() {
        let indices = settings.siteLogins.enumerated().compactMap { index, login in
            selection.contains(login.id) ? index : nil
        }
        settings.deleteSiteLogins(at: IndexSet(indices))
        selection.removeAll()
    }
}

struct LoginEditorSheet: View {
    @Environment(\.dismiss) private var dismiss
    @EnvironmentObject private var settings: AppSettings
    
    let login: SiteLogin?
    
    @State private var urlPattern = ""
    @State private var username = ""
    @State private var password = ""
    
    var body: some View {
        VStack(spacing: 0) {
            Text(login == nil ? "Add Login" : "Edit Login")
                .font(.headline)
                .padding()
            
            Form {
                TextField("URL Pattern (e.g., *.github.com)", text: $urlPattern)
                TextField("Username", text: $username)
                SecureField(login == nil ? "Password" : "Password (leave blank to keep current)", text: $password)
            }
            .padding(.horizontal)
            .padding(.bottom)
            
            if !settings.message.isEmpty && (settings.message.hasPrefix("Add a") || settings.message.hasPrefix("A login") || settings.message.hasPrefix("Could not")) {
                Text(settings.message)
                    .foregroundColor(.red)
                    .font(.caption)
                    .padding(.bottom)
            }
            
            HStack {
                Button("Cancel") {
                    dismiss()
                }
                .keyboardShortcut(.cancelAction)
                
                Spacer()
                
                Button(login == nil ? "Add" : "Save") {
                    settings.message = ""
                    settings.saveSiteLogin(
                        id: login?.id,
                        urlPattern: urlPattern,
                        username: username,
                        password: password
                    )
                    
                    if settings.message.hasPrefix("Added") || settings.message.hasPrefix("Updated") {
                        dismiss()
                    }
                }
                .keyboardShortcut(.defaultAction)
            }
            .padding()
        }
        .frame(width: 400)
        .onAppear {
            if let login = login {
                urlPattern = login.urlPattern
                username = login.username
            }
            settings.message = ""
        }
    }
}

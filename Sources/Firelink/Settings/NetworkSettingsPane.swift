import SwiftUI

struct NetworkSettingsPane: View {
    @EnvironmentObject private var settings: AppSettings

    var body: some View {
        Form {
            Section("Proxy") {
                Picker("Mode", selection: proxyBinding(\.mode)) {
                    ForEach(ProxyMode.allCases, id: \.self) { mode in
                        Text(mode.title)
                            .tag(mode)
                    }
                }
                .pickerStyle(.radioGroup)

                if settings.proxySettings.mode == .custom {
                    Grid(alignment: .leading, horizontalSpacing: 10, verticalSpacing: 10) {
                        GridRow {
                            Text("IP or Host")
                            TextField("127.0.0.1", text: proxyBinding(\.host))
                                .textFieldStyle(.roundedBorder)
                                .font(.system(.body, design: .monospaced))
                        }

                        GridRow {
                            Text("Port")
                            TextField("8080", value: proxyBinding(\.port), format: .number)
                                .textFieldStyle(.roundedBorder)
                                .frame(width: 110)
                        }
                    }
                }

                Text(networkSummary)
                    .font(.caption)
                    .foregroundStyle(.secondary)

                if settings.proxySettings.mode == .custom {
                    Text("aria2 uses an HTTP-style proxy for all protocols. SOCKS proxies are not supported by aria2.")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }

            Section("Identity") {
                TextField("User Agent", text: $settings.customUserAgent, prompt: Text("e.g. Mozilla/5.0..."))
                    .textFieldStyle(.roundedBorder)
                    .font(.system(.body, design: .monospaced))
                
                Text("Spoofs the browser User-Agent to bypass download restrictions. Leave blank for default.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
        }
        .formStyle(.grouped)
    }

    private var networkSummary: String {
        switch settings.proxySettings.mode {
        case .none:
            return "Downloads ignore configured proxies."
        case .system:
            return "Downloads use the matching macOS system proxy when one is configured."
        case .custom:
            if let proxyURI = settings.proxySettings.customProxyURI {
                return "Downloads use \(proxyURI)."
            } else {
                return "Enter a proxy host and port to enable the custom proxy."
            }
        }
    }

    private func proxyBinding<Value>(_ keyPath: WritableKeyPath<ProxySettings, Value>) -> Binding<Value> {
        Binding {
            settings.proxySettings[keyPath: keyPath]
        } set: { newValue in
            var proxySettings = settings.proxySettings
            proxySettings[keyPath: keyPath] = newValue
            settings.proxySettings = proxySettings
        }
    }
}

import SwiftUI

struct NetworkSettingsPane: View {
    @EnvironmentObject private var settings: AppSettings

    var body: some View {
        Form {
            Section {
                Picker("Proxy", selection: proxyBinding(\.mode)) {
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
        }
        .formStyle(.grouped)
    }

    private var networkSummary: String {
        switch settings.proxySettings.mode {
        case .none:
            "Downloads ignore configured proxies."
        case .system:
            "Downloads use the matching macOS system proxy when one is configured."
        case .custom:
            if let proxyURI = settings.proxySettings.customProxyURI {
                "Downloads use \(proxyURI)."
            } else {
                "Enter a proxy host and port to enable the custom proxy."
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

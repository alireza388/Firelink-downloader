import SwiftUI

enum SpeedUnit: String, CaseIterable, Identifiable {
    case kbs = "KB/s"
    case mbs = "MB/s"
    var id: String { rawValue }
}

struct SpeedLimiterView: View {
    @EnvironmentObject private var settings: AppSettings
    @State private var showSaveToast: Bool = false

    // Local state to hold edits before saving
    @State private var isEnabled: Bool = false
    @State private var displayedSpeedValue: Int = 1
    @State private var limitUnit: SpeedUnit = .mbs

    var body: some View {
        VStack(spacing: 0) {
            headerView
            Divider()

            ScrollView {
                VStack(alignment: .leading, spacing: 20) {
                    GroupBox {
                        limitSelectionSection
                            .padding(8)
                    }
                    .opacity(isEnabled ? 1.0 : 0.5)
                    .disabled(!isEnabled)
                }
                .padding(24)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .onAppear {
            loadState()
        }
        .overlay {
            if showSaveToast {
                toastView
                    .transition(.move(edge: .bottom).combined(with: .opacity))
            }
        }
    }

    private var headerView: some View {
        HStack {
            Toggle(isOn: $isEnabled) {
                Text("Speed Limiter")
                    .font(.title2.weight(.bold))
            }
            .toggleStyle(.switch)

            Spacer()

            Button("Save Limit") {
                saveState()
                withAnimation(.spring()) {
                    showSaveToast = true
                }

                DispatchQueue.main.asyncAfter(deadline: .now() + 2.5) {
                    withAnimation {
                        showSaveToast = false
                    }
                }
            }
            .buttonStyle(.borderedProminent)
        }
        .padding(24)
    }

    private var limitSelectionSection: some View {
        VStack(alignment: .leading, spacing: 20) {
            HStack {
                Label("Global Speed Limit", systemImage: "speedometer")
                    .font(.headline)
                Spacer()
            }

            Text("This limit applies globally to all active downloads. Individual downloads can also have their own specific limits defined in their properties.")
                .font(.subheadline)
                .foregroundStyle(.secondary)

            HStack(spacing: 16) {
                TextField("Speed", value: $displayedSpeedValue, format: .number)
                    .textFieldStyle(.roundedBorder)
                    .frame(width: 80)
                    .multilineTextAlignment(.trailing)

                Picker("Unit", selection: $limitUnit) {
                    ForEach(SpeedUnit.allCases) { unit in
                        Text(unit.rawValue).tag(unit)
                    }
                }
                .pickerStyle(.segmented)
                .frame(width: 140)
            }

            Divider()

            VStack(alignment: .leading, spacing: 12) {
                Text("Quick Presets")
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
                
                HStack(spacing: 12) {
                    presetButton(title: "1 MB/s", value: 1, unit: .mbs)
                    presetButton(title: "5 MB/s", value: 5, unit: .mbs)
                    presetButton(title: "10 MB/s", value: 10, unit: .mbs)
                }
            }
        }
    }

    private func presetButton(title: String, value: Int, unit: SpeedUnit) -> some View {
        Button(action: {
            displayedSpeedValue = value
            limitUnit = unit
        }) {
            Text(title)
                .font(.subheadline.weight(.medium))
                .padding(.horizontal, 12)
                .padding(.vertical, 6)
        }
        .buttonStyle(.bordered)
        .controlSize(.regular)
    }

    private var toastView: some View {
        VStack {
            Spacer()
            HStack(spacing: 10) {
                Image(systemName: "checkmark.circle.fill")
                    .foregroundColor(.green)
                Text("Speed Limit Saved")
                    .fontWeight(.medium)
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 10)
            .background(.regularMaterial)
            .clipShape(Capsule())
            .shadow(radius: 4, y: 2)
            .padding(.bottom, 30)
        }
        .allowsHitTesting(false)
    }

    @AppStorage("lastCustomSpeedLimit") private var lastCustomSpeedLimit: Int = 1024

    private func loadState() {
        let currentLimit = settings.globalSpeedLimitKiBPerSecond
        isEnabled = currentLimit > 0
        let effectiveLimit = currentLimit > 0 ? currentLimit : lastCustomSpeedLimit
        
        if effectiveLimit % 1024 == 0 && effectiveLimit >= 1024 {
            displayedSpeedValue = effectiveLimit / 1024
            limitUnit = .mbs
        } else {
            displayedSpeedValue = effectiveLimit
            limitUnit = .kbs
        }
    }

    private func saveState() {
        let valueInKbs = limitUnit == .mbs ? displayedSpeedValue * 1024 : displayedSpeedValue
        let clampedSpeed = max(min(valueInKbs, 10_485_760), 1)

        lastCustomSpeedLimit = clampedSpeed
        settings.globalSpeedLimitKiBPerSecond = isEnabled ? clampedSpeed : 0
    }
}

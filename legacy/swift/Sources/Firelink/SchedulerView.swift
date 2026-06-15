import SwiftUI

struct SchedulerView: View {
    @EnvironmentObject private var downloadController: DownloadController
    @EnvironmentObject private var schedulerController: SchedulerController
    @State private var showSaveToast: Bool = false

    // Local state to hold edits before saving
    @State private var isEnabled: Bool = false
    @State private var startTime: Date = Date()
    @State private var stopTimeEnabled: Bool = false
    @State private var stopTime: Date = Date()
    @State private var isEveryday: Bool = true
    @State private var selectedDays: Set<SchedulerDay> = []
    @State private var postQueueAction: PostQueueAction = .doNothing
    @State private var targetQueueIDs: Set<UUID> = []

    var body: some View {
        VStack(spacing: 0) {
            headerView
            Divider()

            ScrollView {
                VStack(alignment: .leading, spacing: 20) {
                    Group {
                        timeSelectionSection
                        queueSelectionSection
                        postActionSection
                    }
                    .opacity(isEnabled ? 1.0 : 0.5)
                    .disabled(!isEnabled)

                    permissionsSection
                }
                .padding(24)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .onAppear {
            loadState()
            schedulerController.checkAutomationPermission()
        }
        .onReceive(NotificationCenter.default.publisher(for: NSApplication.didBecomeActiveNotification)) { _ in
            schedulerController.checkAutomationPermission()
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
                Text("Scheduler")
                    .font(.title2.weight(.bold))
            }
            .toggleStyle(.switch)

            Spacer()

            Button("Save Settings") {
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

    private var timeSelectionSection: some View {
        GroupBox {
            VStack(alignment: .leading, spacing: 16) {
                HStack {
                    Label("Timing", systemImage: "clock")
                        .font(.headline)
                    Spacer()
                }

                HStack(spacing: 24) {
                    VStack(alignment: .leading, spacing: 8) {
                        Text("Start Time")
                            .foregroundStyle(.secondary)
                        DatePicker("Start", selection: $startTime, displayedComponents: [.hourAndMinute])
                            .datePickerStyle(.stepperField)
                            .labelsHidden()
                    }

                    VStack(alignment: .leading, spacing: 8) {
                        Toggle("Stop Time", isOn: $stopTimeEnabled)
                            .foregroundStyle(.secondary)
                        DatePicker("Stop", selection: $stopTime, displayedComponents: [.hourAndMinute])
                            .datePickerStyle(.stepperField)
                            .labelsHidden()
                            .disabled(!stopTimeEnabled)
                            .opacity(stopTimeEnabled ? 1.0 : 0.5)
                    }
                }

                Divider()

                Toggle("Run Every Day", isOn: $isEveryday)

                if !isEveryday {
                    HStack(spacing: 8) {
                        ForEach(SchedulerDay.allCases) { day in
                            Button(action: {
                                if selectedDays.contains(day) {
                                    selectedDays.remove(day)
                                } else {
                                    selectedDays.insert(day)
                                }
                            }) {
                                Text(day.shortName)
                                    .fontWeight(.medium)
                                    .frame(width: 28, height: 28)
                            }
                            .buttonStyle(.borderless)
                            .background(selectedDays.contains(day) ? Color.accentColor : Color(nsColor: .controlBackgroundColor))
                            .foregroundStyle(selectedDays.contains(day) ? Color.white : Color.primary)
                            .clipShape(Circle())
                        }
                    }
                }
            }
            .padding(8)
        }
    }

    private var queueSelectionSection: some View {
        GroupBox {
            VStack(alignment: .leading, spacing: 16) {
                HStack {
                    Label("Queues to Schedule", systemImage: "list.bullet.rectangle")
                        .font(.headline)
                    Spacer()
                }

                if downloadController.queues.isEmpty {
                    Text("No queues available")
                        .foregroundStyle(.secondary)
                } else {
                    VStack(alignment: .leading, spacing: 12) {
                        ForEach(downloadController.queues) { queue in
                            Toggle(queue.name, isOn: Binding(
                                get: { targetQueueIDs.contains(queue.id) },
                                set: { isSelected in
                                    if isSelected {
                                        targetQueueIDs.insert(queue.id)
                                    } else {
                                        targetQueueIDs.remove(queue.id)
                                    }
                                }
                            ))
                        }
                    }
                }
            }
            .padding(8)
        }
    }

    private var postActionSection: some View {
        GroupBox {
            VStack(alignment: .leading, spacing: 16) {
                HStack {
                    Label("After Completion", systemImage: "powersleep")
                        .font(.headline)
                    Spacer()
                }

                Text("Choose what happens after scheduled downloads finish.")
                    .font(.subheadline)
                    .foregroundStyle(.secondary)

                Picker("Action", selection: $postQueueAction) {
                    ForEach(PostQueueAction.allCases) { action in
                        Text(action.rawValue).tag(action)
                    }
                }
                .labelsHidden()
                .pickerStyle(.radioGroup)
            }
            .padding(8)
        }
    }

    private var permissionsSection: some View {
        GroupBox {
            VStack(alignment: .leading, spacing: 12) {
                HStack {
                    Label("System Permissions", systemImage: "lock.shield")
                        .font(.headline)
                    Spacer()
                }

                Text("Firelink needs Automation permission to control Finder in order to automatically sleep, restart, or shut down your Mac after downloads finish.")
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)

                if schedulerController.hasAutomationPermission {
                    Button("Revoke Permissions") {
                        schedulerController.openAutomationPermissionSettings()
                    }
                    .buttonStyle(.bordered)
                } else {
                    Button("Grant Permission") {
                        schedulerController.requestAutomationPermission()
                    }
                    .buttonStyle(.borderedProminent)
                }
            }
            .padding(8)
        }
    }

    private var toastView: some View {
        VStack {
            Spacer()
            HStack(spacing: 10) {
                Image(systemName: "checkmark.circle.fill")
                    .foregroundColor(.green)
                Text("Settings Saved")
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

    private func loadState() {
        isEnabled = schedulerController.settings.isEnabled
        startTime = schedulerController.settings.startTime
        stopTimeEnabled = schedulerController.settings.stopTimeEnabled
        stopTime = schedulerController.settings.stopTime
        isEveryday = schedulerController.settings.isEveryday
        selectedDays = schedulerController.settings.selectedDays
        postQueueAction = schedulerController.settings.postQueueAction
        targetQueueIDs = schedulerController.settings.targetQueueIDs.isEmpty
            ? [DownloadQueue.mainQueueID]
            : schedulerController.settings.targetQueueIDs
    }

    private func saveState() {
        schedulerController.settings.isEnabled = isEnabled
        schedulerController.settings.startTime = startTime
        schedulerController.settings.stopTimeEnabled = stopTimeEnabled
        schedulerController.settings.stopTime = stopTime
        schedulerController.settings.isEveryday = isEveryday
        schedulerController.settings.selectedDays = selectedDays
        schedulerController.settings.postQueueAction = postQueueAction
        schedulerController.settings.targetQueueIDs = targetQueueIDs.isEmpty
            ? [DownloadQueue.mainQueueID]
            : targetQueueIDs
        schedulerController.saveSettings()
    }
}

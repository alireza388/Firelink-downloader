import re

with open("Sources/Firelink/DownloadTable.swift", "r") as f:
    content = f.read()

# Fix size to use bytesText if sizeBytes is nil or 0
old_size = '''                TableColumn("Size", value: \\.sortableSize) { item in
                    Text(ByteFormatter.string(item.sizeBytes))
                            .monospacedDigit()
                            .lineLimit(1)
                            .truncationMode(.tail)
                }'''
new_size = '''                TableColumn("Size", value: \\.sortableSize) { item in
                    if let size = item.sizeBytes, size > 0 {
                        Text(ByteFormatter.string(size))
                            .monospacedDigit()
                            .lineLimit(1)
                            .truncationMode(.tail)
                    } else if item.bytesText != "-" && !item.bytesText.isEmpty {
                        Text(item.bytesText)
                            .monospacedDigit()
                            .lineLimit(1)
                            .truncationMode(.tail)
                    } else {
                        Text("Unknown")
                            .monospacedDigit()
                            .lineLimit(1)
                            .truncationMode(.tail)
                    }
                }'''
content = content.replace(old_size, new_size)

# Add allowsHitTesting(false) to text to fix single click interception
old_text = '''                            Text(item.fileName)
                                .font(.headline)
                                .lineLimit(1)
                                .truncationMode(.tail)'''
new_text = '''                            Text(item.fileName)
                                .font(.headline)
                                .lineLimit(1)
                                .truncationMode(.tail)
                                .allowsHitTesting(false)'''
content = content.replace(old_text, new_text)

# Change performPrimaryAction to use id
old_action = 'openWindow(value: item.id)'
new_action = 'openWindow(id: "download-properties", value: item.id)'
content = content.replace(old_action, new_action)

with open("Sources/Firelink/DownloadTable.swift", "w") as f:
    f.write(content)

with open("Sources/Firelink/FirelinkApp.swift", "r") as f:
    app_content = f.read()

app_content = app_content.replace(
    'WindowGroup("Download Properties", for: UUID.self)',
    'WindowGroup("Download Properties", id: "download-properties", for: UUID.self)'
)

with open("Sources/Firelink/FirelinkApp.swift", "w") as f:
    f.write(app_content)

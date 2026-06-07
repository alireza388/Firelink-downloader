with open("Sources/Firelink/FirelinkApp.swift", "r") as f:
    app_content = f.read()

app_content = app_content.replace(
    'WindowGroup("Download Properties", id: "download-properties", for: String.self) { $downloadIDString in\n            if let idString = downloadIDString, let downloadID = UUID(uuidString: idString) {',
    'WindowGroup("Download Properties", id: "download-properties", for: UUID.self) { $downloadID in\n            if let downloadID {'
)

with open("Sources/Firelink/FirelinkApp.swift", "w") as f:
    f.write(app_content)

with open("Sources/Firelink/DownloadTable.swift", "r") as f:
    table_content = f.read()

# Fix openWindow in performPrimaryAction
table_content = table_content.replace(
    'openWindow(id: "download-properties", value: item.id.uuidString)',
    'openWindow(id: "download-properties", value: item.id)'
)

# Fix openWindow in rowContextMenu
table_content = table_content.replace(
    'openWindow(value: target.id)',
    'openWindow(id: "download-properties", value: target.id)'
)

# Remove simultaneousGesture from Table
import re
table_content = re.sub(
    r'\s*\.simultaneousGesture\(TapGesture\(count: 2\)\.onEnded \{\s*if let id = selection\.first, let item = controller\.downloads\.first\(where: \{ \$0\.id == id \}\) \{\s*performPrimaryAction\(for: item\)\s*\}\s*\}\)',
    '',
    table_content
)

# Replace the File Name TableColumn to use doubleClickableCell
old_column = '''                TableColumn("File Name", value: \\.fileName) { item in
                    HStack(alignment: .top, spacing: 8) {
                            Image(systemName: item.category.symbolName)
                                .font(.title3)
                                .foregroundStyle(categoryColor(for: item.category))
                                .frame(width: 22)
                            Text(item.fileName)
                                .font(.headline)
                                .lineLimit(1)
                                .truncationMode(.tail)
                                .allowsHitTesting(false)
                        .draggable(item.id.uuidString)
                    }
                }'''

new_column = '''                TableColumn("File Name", value: \\.fileName) { item in
                    doubleClickableCell(for: item) {
                        HStack(alignment: .top, spacing: 8) {
                            Image(systemName: item.category.symbolName)
                                .font(.title3)
                                .foregroundStyle(categoryColor(for: item.category))
                                .frame(width: 22)
                            Text(item.fileName)
                                .font(.headline)
                                .lineLimit(1)
                                .truncationMode(.tail)
                                .allowsHitTesting(false)
                        }
                        .draggable(item.id.uuidString)
                    }
                }'''

table_content = table_content.replace(old_column, new_column)

# Add doubleClickableCell helper
helper = '''    private func performPrimaryAction(for item: DownloadItem) {'''

helper_new = '''    private func doubleClickableCell<Content: View>(for item: DownloadItem, @ViewBuilder content: () -> Content) -> some View {
        content()
            .contentShape(Rectangle())
            .simultaneousGesture(TapGesture(count: 2).onEnded {
                performPrimaryAction(for: item)
            })
    }

    private func performPrimaryAction(for item: DownloadItem) {'''

table_content = table_content.replace(helper, helper_new)

with open("Sources/Firelink/DownloadTable.swift", "w") as f:
    f.write(table_content)

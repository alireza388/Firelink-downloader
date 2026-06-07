import re

with open("Sources/Firelink/DownloadTable.swift", "r") as f:
    content = f.read()

# Remove doubleClickableCell block
content = re.sub(r'    private func doubleClickableCell.*?    }\n\n', '', content, flags=re.DOTALL)

# Remove doubleClickableCell wrappers from TableColumn
content = re.sub(r'doubleClickableCell\(for: item\) \{\n\s*(.*?)\n\s*\}', r'\1', content, flags=re.DOTALL)

# Add simultaneousGesture to Table
table_end = content.find('            .environment(\\.defaultMinListRowHeight, settings.listRowDensity.minRowHeight)')
gesture = '''            .simultaneousGesture(TapGesture(count: 2).onEnded {
                if let id = selection.first, let item = controller.downloads.first(where: { $0.id == id }) {
                    performPrimaryAction(for: item)
                }
            })
'''
content = content[:table_end] + gesture + content[table_end:]

with open("Sources/Firelink/DownloadTable.swift", "w") as f:
    f.write(content)
